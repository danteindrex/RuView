/**
 * @file ftm_ranging.c
 * @brief 802.11 FTM ranging — ADR-091 (initiator + on-demand responder).
 *
 * Initiator: esp_wifi_ftm_initiate_session() on the current channel,
 * result harvested from WIFI_EVENT_FTM_REPORT and forwarded to the
 * aggregator as a 24-byte little-endian packet (magic 0xC5110008).
 * Sessions are serialized (one in flight) with a ~4 s timeout guard.
 *
 * Responder: ESP-IDF only answers FTM requests from SoftAP mode, so
 * enabling the responder switches STA -> APSTA with a hidden, randomly
 * passworded SoftAP (SSID "RV-FTM-<node_id>") carrying ftm_responder=true.
 * The SoftAP shares the single radio and therefore sits on the STA's
 * channel. Coexistence with the continuous CSI capture path has NOT yet
 * been validated on hardware, so the responder defaults to OFF and is
 * enabled on demand ("WAVE_FTM_RESPONDER|on") or via NVS "ftm_resp"=1.
 *
 * Concurrency: session state is guarded by a FreeRTOS mutex taken from
 * three task contexts — the discovery task (session start), the esp_timer
 * task (timeout guard), and the default event-loop task (FTM report).
 * The timeout timer is armed and stopped only while holding the mutex and
 * only by the owner of the active session, so a late report/timeout for a
 * finished session can never kill the guard of a newer one.
 */

#include "ftm_ranging.h"

#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/semphr.h"
#include "esp_log.h"
#include "esp_wifi.h"
#include "esp_event.h"
#include "esp_timer.h"
#include "esp_random.h"
#include "esp_netif.h"
#include "sdkconfig.h"

#include "nvs_config.h"
#include "csi_collector.h"
#include "stream_sender.h"

static const char *TAG = "ftm_ranging";

/** Session timeout guard: FTM report must arrive within this window. */
#define FTM_SESSION_TIMEOUT_MS 4000

/** FTM frames per burst (valid: 0/16/24/32/64; 16 keeps airtime short). */
#define FTM_FRAME_COUNT 16

/** Burst period in units of 100 ms (2 = 200 ms between bursts). */
#define FTM_BURST_PERIOD 2

/* Runtime configuration (owned by main.c). */
extern nvs_config_t g_nvs_config;

/* ---- Session state (guarded by s_mutex) ---- */
static SemaphoreHandle_t s_mutex = NULL;
static bool    s_session_active = false;
static uint8_t s_peer_mac[6];

static bool s_initialized       = false;
static bool s_responder_enabled = false;
static bool s_ap_netif_created  = false;
static esp_timer_handle_t s_timeout_timer = NULL;
static esp_event_handler_instance_t s_ftm_evt_instance = NULL;

/**
 * Build and send the 24-byte range report (ADR-091, magic 0xC5110008) to
 * the aggregator via the existing CSI stream socket.
 *
 * Layout (little-endian; ESP32 is LE so memcpy of scalars is canonical):
 *   [0..3]   magic u32 = 0xC5110008
 *   [4]      node_id u8
 *   [5]      status u8 (0=ok, 1=failed/timeout, 2=unsupported, 3=busy)
 *   [6..11]  peer MAC
 *   [12..15] distance_cm u32 (0 when status != 0)
 *   [16..19] rtt_est_ns u32
 *   [20]     num_frames u8 (FTM frames used)
 *   [21..23] reserved = 0
 */
static void send_range_report(const uint8_t peer_mac[6], uint8_t status,
                              uint32_t distance_cm, uint32_t rtt_est_ns,
                              uint8_t num_frames)
{
    uint8_t pkt[FTM_RANGE_REPORT_SIZE] = {0};
    uint32_t magic = FTM_RANGE_MAGIC;

    if (status != FTM_RANGE_STATUS_OK) {
        distance_cm = 0;
    }

    memcpy(&pkt[0], &magic, 4);
    pkt[4] = csi_collector_get_node_id();
    pkt[5] = status;
    memcpy(&pkt[6], peer_mac, 6);
    memcpy(&pkt[12], &distance_cm, 4);
    memcpy(&pkt[16], &rtt_est_ns, 4);
    pkt[20] = num_frames;
    /* pkt[21..23] reserved, already zeroed. */

    if (stream_sender_send(pkt, sizeof(pkt)) < 0) {
        ESP_LOGW(TAG, "Range report send failed (status=%u)", (unsigned)status);
    }
}

/** Timeout guard — the FTM report never arrived. */
static void ftm_timeout_cb(void *arg)
{
    (void)arg;
    uint8_t mac[6];

    xSemaphoreTake(s_mutex, portMAX_DELAY);
    if (!s_session_active) {
        /* Report already handled between timer expiry and this callback. */
        xSemaphoreGive(s_mutex);
        return;
    }
    memcpy(mac, s_peer_mac, 6);
    s_session_active = false;
    esp_wifi_ftm_end_session();
    xSemaphoreGive(s_mutex);

    ESP_LOGW(TAG, "FTM session to %02X:%02X:%02X:%02X:%02X:%02X timed out (%d ms)",
             mac[0], mac[1], mac[2], mac[3], mac[4], mac[5],
             FTM_SESSION_TIMEOUT_MS);
    send_range_report(mac, FTM_RANGE_STATUS_FAILED, 0, 0, 0);
}

/** WIFI_EVENT_FTM_REPORT handler (default event-loop task). */
static void ftm_report_handler(void *arg, esp_event_base_t event_base,
                               int32_t event_id, void *event_data)
{
    (void)arg;
    (void)event_base;
    (void)event_id;
    wifi_event_ftm_report_t *ev = (wifi_event_ftm_report_t *)event_data;

    uint8_t mac[6];
    xSemaphoreTake(s_mutex, portMAX_DELAY);
    bool owned = s_session_active && memcmp(ev->peer_mac, s_peer_mac, 6) == 0;
    if (owned) {
        memcpy(mac, s_peer_mac, 6);
        s_session_active = false;
        esp_timer_stop(s_timeout_timer);
    }
    xSemaphoreGive(s_mutex);

    /* Per-frame report entries are heap-allocated by the WiFi driver and
     * owned by the application once the event fires. */
    if (ev->ftm_report_data != NULL) {
        free(ev->ftm_report_data);
        ev->ftm_report_data = NULL;
    }

    if (!owned) {
        /* Stale report (timed-out or terminated session) — already reported. */
        return;
    }

    uint8_t status;
    switch (ev->status) {
    case FTM_STATUS_SUCCESS:
        status = FTM_RANGE_STATUS_OK;
        break;
    case FTM_STATUS_UNSUPPORTED:
        status = FTM_RANGE_STATUS_UNSUPPORTED;
        break;
    default:
        status = FTM_RANGE_STATUS_FAILED;
        break;
    }

    if (status == FTM_RANGE_STATUS_OK) {
        ESP_LOGI(TAG, "FTM report %02X:%02X:%02X:%02X:%02X:%02X: dist=%lu cm, rtt=%lu ns, frames=%u",
                 mac[0], mac[1], mac[2], mac[3], mac[4], mac[5],
                 (unsigned long)ev->dist_est, (unsigned long)ev->rtt_est,
                 (unsigned)ev->ftm_report_num_entries);
    } else {
        ESP_LOGW(TAG, "FTM session %02X:%02X:%02X:%02X:%02X:%02X failed: driver status=%d",
                 mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], (int)ev->status);
    }
    send_range_report(mac, status, ev->dist_est, ev->rtt_est,
                      ev->ftm_report_num_entries);
}

/** Resolve the channel to range on: live radio channel, else NVS config. */
static uint8_t current_channel(void)
{
    uint8_t primary = 0;
    wifi_second_chan_t second;
    if (esp_wifi_get_channel(&primary, &second) == ESP_OK && primary != 0) {
        return primary;
    }
    if (g_nvs_config.csi_channel != 0) {
        return g_nvs_config.csi_channel;
    }
    return g_nvs_config.channel_list[0];
}

esp_err_t ftm_ranging_start_session(const uint8_t peer_mac[6])
{
    if (!s_initialized || peer_mac == NULL) {
        return ESP_ERR_INVALID_STATE;
    }

    /* Serialize: one session in flight at a time. Claim, initiate, and arm
     * the timeout guard under the mutex so the report handler (which also
     * takes the mutex) cannot interleave with a half-started session. */
    xSemaphoreTake(s_mutex, portMAX_DELAY);
    if (s_session_active) {
        xSemaphoreGive(s_mutex);
        ESP_LOGW(TAG, "FTM session request rejected: another session in flight");
        send_range_report(peer_mac, FTM_RANGE_STATUS_BUSY, 0, 0, 0);
        return ESP_ERR_INVALID_STATE;
    }
    s_session_active = true;
    memcpy(s_peer_mac, peer_mac, 6);

    wifi_ftm_initiator_cfg_t cfg;
    memset(&cfg, 0, sizeof(cfg));
    memcpy(cfg.resp_mac, peer_mac, 6);
    cfg.channel      = current_channel();
    cfg.frm_count    = FTM_FRAME_COUNT;
    cfg.burst_period = FTM_BURST_PERIOD;

    ESP_LOGI(TAG, "FTM session start -> %02X:%02X:%02X:%02X:%02X:%02X (channel %u, %u frames)",
             peer_mac[0], peer_mac[1], peer_mac[2],
             peer_mac[3], peer_mac[4], peer_mac[5],
             (unsigned)cfg.channel, (unsigned)cfg.frm_count);

    esp_err_t err = esp_wifi_ftm_initiate_session(&cfg);
    if (err != ESP_OK) {
        s_session_active = false;
        xSemaphoreGive(s_mutex);
        uint8_t status = (err == ESP_ERR_NOT_SUPPORTED)
                             ? FTM_RANGE_STATUS_UNSUPPORTED
                             : FTM_RANGE_STATUS_FAILED;
        ESP_LOGW(TAG, "FTM initiate failed: %s", esp_err_to_name(err));
        send_range_report(peer_mac, status, 0, 0, 0);
        return err;
    }

    err = esp_timer_start_once(s_timeout_timer,
                               (uint64_t)FTM_SESSION_TIMEOUT_MS * 1000ULL);
    if (err != ESP_OK) {
        /* Without the timeout guard a lost report would leave the node
         * permanently "busy" — abort the session instead. */
        esp_wifi_ftm_end_session();
        s_session_active = false;
        xSemaphoreGive(s_mutex);
        ESP_LOGE(TAG, "FTM timeout timer arm failed: %s", esp_err_to_name(err));
        send_range_report(peer_mac, FTM_RANGE_STATUS_FAILED, 0, 0, 0);
        return err;
    }

    xSemaphoreGive(s_mutex);
    return ESP_OK;
}

esp_err_t ftm_ranging_set_responder(bool enable)
{
    if (!s_initialized) {
        return ESP_ERR_INVALID_STATE;
    }
    if (enable == s_responder_enabled) {
        return ESP_OK;
    }

    if (enable) {
        /* A SoftAP cannot follow the ADR-073 hop table — refuse rather
         * than beacon on a stale channel. */
        if (g_nvs_config.channel_hop_count > 1) {
            ESP_LOGW(TAG, "FTM responder refused: channel hopping active");
            return ESP_ERR_INVALID_STATE;
        }

        if (!s_ap_netif_created) {
            esp_netif_create_default_wifi_ap();
            s_ap_netif_created = true;
        }

        esp_err_t err = esp_wifi_set_mode(WIFI_MODE_APSTA);
        if (err != ESP_OK) {
            ESP_LOGE(TAG, "APSTA mode switch failed: %s", esp_err_to_name(err));
            return err;
        }

        wifi_config_t ap_cfg;
        memset(&ap_cfg, 0, sizeof(ap_cfg));
        int n = snprintf((char *)ap_cfg.ap.ssid, sizeof(ap_cfg.ap.ssid),
                         "RV-FTM-%u", (unsigned)csi_collector_get_node_id());
        ap_cfg.ap.ssid_len       = (uint8_t)n;  /* node_id is u8 -> max 10 chars. */
        ap_cfg.ap.channel        = current_channel();
        ap_cfg.ap.ssid_hidden    = 1;
        ap_cfg.ap.max_connection = 1;
        ap_cfg.ap.authmode       = WIFI_AUTH_WPA2_PSK;
        ap_cfg.ap.ftm_responder  = true;
        /* Nothing should ever associate — random throwaway password
         * (32 hex chars + NUL). */
        _Static_assert(sizeof(ap_cfg.ap.password) >= 33,
                       "AP password field too small for 32 hex chars + NUL");
        for (int i = 0; i < 4; i++) {
            snprintf((char *)&ap_cfg.ap.password[i * 8], 9, "%08lx",
                     (unsigned long)esp_random());
        }

        err = esp_wifi_set_config(WIFI_IF_AP, &ap_cfg);
        if (err != ESP_OK) {
            ESP_LOGE(TAG, "FTM responder AP config failed: %s", esp_err_to_name(err));
            if (esp_wifi_set_mode(WIFI_MODE_STA) != ESP_OK) {
                ESP_LOGE(TAG, "STA mode restore failed after AP config error");
            }
            return err;
        }

        s_responder_enabled = true;
        ESP_LOGI(TAG, "FTM responder ON (hidden SoftAP \"RV-FTM-%u\", channel %u)",
                 (unsigned)csi_collector_get_node_id(), (unsigned)ap_cfg.ap.channel);
    } else {
        esp_err_t err = esp_wifi_set_mode(WIFI_MODE_STA);
        if (err != ESP_OK) {
            ESP_LOGE(TAG, "STA mode restore failed: %s", esp_err_to_name(err));
            return err;
        }
        s_responder_enabled = false;
        ESP_LOGI(TAG, "FTM responder OFF (back to STA-only)");
    }
    return ESP_OK;
}

esp_err_t ftm_ranging_init(void)
{
    if (s_initialized) {
        return ESP_OK;
    }

    s_mutex = xSemaphoreCreateMutex();
    if (s_mutex == NULL) {
        return ESP_ERR_NO_MEM;
    }

    esp_timer_create_args_t timer_args = {
        .callback        = ftm_timeout_cb,
        .arg             = NULL,
        .dispatch_method = ESP_TIMER_TASK,
        .name            = "ftm_timeout",
    };
    esp_err_t err = esp_timer_create(&timer_args, &s_timeout_timer);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Timeout timer create failed: %s", esp_err_to_name(err));
        vSemaphoreDelete(s_mutex);
        s_mutex = NULL;
        return err;
    }

    err = esp_event_handler_instance_register(WIFI_EVENT, WIFI_EVENT_FTM_REPORT,
                                              &ftm_report_handler, NULL,
                                              &s_ftm_evt_instance);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "FTM report handler register failed: %s", esp_err_to_name(err));
        esp_timer_delete(s_timeout_timer);
        s_timeout_timer = NULL;
        vSemaphoreDelete(s_mutex);
        s_mutex = NULL;
        return err;
    }

    s_initialized = true;

    /* ADR-091: responder default is OFF (coexistence with CSI capture is
     * pending hardware validation); opt in via NVS "ftm_resp"=1. */
    if (g_nvs_config.ftm_responder) {
        if (ftm_ranging_set_responder(true) != ESP_OK) {
            ESP_LOGW(TAG, "NVS ftm_resp=1 but responder enable failed");
        }
    }

    ESP_LOGI(TAG, "FTM ranging ready (responder=%s)",
             s_responder_enabled ? "on" : "off");
    return ESP_OK;
}
