/**
 * @file discovery_responder.c
 * @brief UDP discovery responder for the Wave desktop console (port 5006).
 *
 * Answers RUVIEW_DISCOVER broadcasts with a RUVIEW_BEACON reply so the
 * desktop app can register this node without knowing its IP.  The same
 * socket accepts "RUVIEW_HUB|<ip>|<port>" announcements from the sensing
 * server and re-targets the CSI stream at runtime -- this heals nodes when
 * the hub PC receives a new DHCP lease.  The new target is persisted to
 * NVS so it survives reboot.
 *
 * Beacon field order MUST match parse_beacon_response in
 * wifi-densepose-desktop/src/commands/discovery.rs:
 *   RUVIEW_BEACON|<mac>|<node_id>|<version>|<chip>|<role>|<tdm_slot>|<tdm_total>
 */

#include "discovery_responder.h"

#include <string.h>
#include <stdlib.h>
#include <stdio.h>
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "esp_log.h"
#include "esp_wifi.h"
#include "esp_app_desc.h"
#include "nvs_flash.h"
#include "nvs.h"
#include "lwip/sockets.h"
#include "sdkconfig.h"

#include "nvs_config.h"
#include "stream_sender.h"

static const char *TAG = "discovery";

#define DISCOVERY_TASK_STACK 4096
#define DISCOVERY_TASK_PRIO  4
#define DISCOVERY_TASK_CORE  1   /* Pin to APP CPU, away from the WiFi core. */

/* Runtime configuration (owned by main.c). */
extern nvs_config_t g_nvs_config;

/**
 * Persist a hub re-announcement to NVS so the new target survives reboot.
 * Uses the same "csi_cfg" namespace / keys that nvs_config_load() reads.
 */
static void persist_target(const char *ip, uint16_t port)
{
    nvs_handle_t handle;
    esp_err_t err = nvs_open("csi_cfg", NVS_READWRITE, &handle);
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "NVS open for hub persist failed: %s", esp_err_to_name(err));
        return;
    }
    if (nvs_set_str(handle, "target_ip", ip) == ESP_OK &&
        nvs_set_u16(handle, "target_port", port) == ESP_OK &&
        nvs_commit(handle) == ESP_OK) {
        ESP_LOGI(TAG, "Hub target persisted to NVS: %s:%u", ip, (unsigned)port);
    } else {
        ESP_LOGW(TAG, "Failed to persist hub target to NVS");
    }
    nvs_close(handle);
}

/**
 * Handle a "RUVIEW_HUB|<ip>|<port>" announcement (ADR-032 note: the LAN is
 * trusted here -- same trust boundary as OTA/WASM upload HTTP endpoints).
 * @param msg NUL-terminated datagram text (modified in place by strtok_r).
 */
static void handle_hub_announce(char *msg)
{
    char *saveptr = NULL;
    strtok_r(msg, "|", &saveptr);              /* Skip "RUVIEW_HUB". */
    char *ip = strtok_r(NULL, "|", &saveptr);
    char *port_str = strtok_r(NULL, "|", &saveptr);
    if (ip == NULL || port_str == NULL) {
        ESP_LOGW(TAG, "Malformed RUVIEW_HUB announcement");
        return;
    }

    char *end = NULL;
    long port = strtol(port_str, &end, 10);
    if (end == port_str || port <= 0 || port > 65535 || strlen(ip) >= NVS_CFG_IP_MAX) {
        ESP_LOGW(TAG, "Invalid hub announcement: %s:%s", ip, port_str);
        return;
    }

    if (strcmp(ip, g_nvs_config.target_ip) == 0 &&
        (uint16_t)port == g_nvs_config.target_port) {
        return;  /* Already streaming to this target. */
    }

    if (stream_sender_retarget(ip, (uint16_t)port) != 0) {
        ESP_LOGW(TAG, "Hub re-target rejected: %s:%ld", ip, port);
        return;
    }

    strncpy(g_nvs_config.target_ip, ip, NVS_CFG_IP_MAX - 1);
    g_nvs_config.target_ip[NVS_CFG_IP_MAX - 1] = '\0';
    g_nvs_config.target_port = (uint16_t)port;
    ESP_LOGW(TAG, "Hub re-announced -- CSI stream re-targeted to %s:%ld", ip, port);
    persist_target(ip, (uint16_t)port);
}

/** Build the RUVIEW_BEACON reply from live node state. */
static int build_beacon(char *out, size_t out_len)
{
    uint8_t mac[6] = {0};
    esp_wifi_get_mac(WIFI_IF_STA, mac);
    const esp_app_desc_t *app = esp_app_get_description();

    return snprintf(out, out_len,
        "RUVIEW_BEACON|%02X:%02X:%02X:%02X:%02X:%02X|%u|%s|%s|node|%u|%u",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5],
        (unsigned)g_nvs_config.node_id,
        app->version,
        CONFIG_IDF_TARGET,
        (unsigned)g_nvs_config.tdm_slot_index,
        (unsigned)g_nvs_config.tdm_node_count);
}

static void discovery_task(void *arg)
{
    (void)arg;

    int sock = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
    if (sock < 0) {
        ESP_LOGE(TAG, "Failed to create discovery socket: errno %d", errno);
        vTaskDelete(NULL);
        return;
    }

    int reuse = 1;
    setsockopt(sock, SOL_SOCKET, SO_REUSEADDR, &reuse, sizeof(reuse));

    struct sockaddr_in bind_addr;
    memset(&bind_addr, 0, sizeof(bind_addr));
    bind_addr.sin_family = AF_INET;
    bind_addr.sin_addr.s_addr = htonl(INADDR_ANY);
    bind_addr.sin_port = htons(DISCOVERY_RESPONDER_PORT);
    if (bind(sock, (struct sockaddr *)&bind_addr, sizeof(bind_addr)) < 0) {
        ESP_LOGE(TAG, "Failed to bind UDP :%d: errno %d", DISCOVERY_RESPONDER_PORT, errno);
        close(sock);
        vTaskDelete(NULL);
        return;
    }

    ESP_LOGI(TAG, "Discovery responder listening on UDP :%d", DISCOVERY_RESPONDER_PORT);

    char rx[128];
    char beacon[128];
    while (1) {
        struct sockaddr_in src;
        socklen_t src_len = sizeof(src);
        int len = recvfrom(sock, rx, sizeof(rx) - 1, 0,
                           (struct sockaddr *)&src, &src_len);
        if (len < 0) {
            ESP_LOGW(TAG, "recvfrom failed: errno %d", errno);
            vTaskDelay(pdMS_TO_TICKS(100));
            continue;
        }
        rx[len] = '\0';

        if (strncmp(rx, "RUVIEW_DISCOVER", 15) == 0) {
            int n = build_beacon(beacon, sizeof(beacon));
            if (n > 0 && n < (int)sizeof(beacon)) {
                if (sendto(sock, beacon, n, 0,
                           (struct sockaddr *)&src, src_len) < 0) {
                    ESP_LOGW(TAG, "Beacon reply failed: errno %d", errno);
                }
            }
        } else if (strncmp(rx, "RUVIEW_HUB|", 11) == 0) {
            handle_hub_announce(rx);
        }
        /* Anything else: silently ignore (unknown broadcast traffic). */
    }
}

esp_err_t discovery_responder_start(void)
{
    BaseType_t ok = xTaskCreatePinnedToCore(discovery_task, "discovery",
                                            DISCOVERY_TASK_STACK, NULL,
                                            DISCOVERY_TASK_PRIO, NULL,
                                            DISCOVERY_TASK_CORE);
    if (ok != pdPASS) {
        ESP_LOGE(TAG, "Failed to create discovery task");
        return ESP_ERR_NO_MEM;
    }
    return ESP_OK;
}
