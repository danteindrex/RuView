/**
 * @file ftm_ranging.h
 * @brief 802.11 FTM (Fine Timing Measurement) ranging — ADR-091.
 *
 * Lets the sensing server command node-to-node distance measurements:
 *  - Initiator: "RUVIEW_RANGE|<mac>" on UDP :5006 starts an FTM session to
 *    the peer on the current channel; the result is reported to the
 *    aggregator (:5005) as a 24-byte packet with magic 0xC5110008.
 *  - Responder: "RUVIEW_FTM_RESPONDER|on/off" toggles a hidden SoftAP
 *    (APSTA mode) with ftm_responder=true so peers can range against us.
 *
 * Responder default is OFF (conservative): the node's primary job is
 * continuous CSI capture in STA mode, and the STA->APSTA switch carries a
 * coexistence risk with the CSI callback that is pending hardware
 * validation (see ADR-091). Enable at boot via NVS key "ftm_resp"=1.
 */

#ifndef FTM_RANGING_H
#define FTM_RANGING_H

#include <stdint.h>
#include <stdbool.h>
#include "esp_err.h"

/** ADR-091 FTM range report packet magic (next after 0xC5110007 WASM v2). */
#define FTM_RANGE_MAGIC 0xC5110008

/** Fixed size of the range report packet (bytes, little-endian). */
#define FTM_RANGE_REPORT_SIZE 24

/** Range report status codes (byte 5 of the report packet). */
#define FTM_RANGE_STATUS_OK          0  /**< Measurement succeeded. */
#define FTM_RANGE_STATUS_FAILED      1  /**< Session failed or timed out. */
#define FTM_RANGE_STATUS_UNSUPPORTED 2  /**< Peer unsupported / responder off. */
#define FTM_RANGE_STATUS_BUSY        3  /**< A session is already in flight. */

/**
 * Initialize FTM ranging.
 *
 * Registers the WIFI_EVENT_FTM_REPORT handler and creates the session
 * timeout timer. If NVS key "ftm_resp" is 1, also enables responder mode.
 *
 * Must be called after WiFi is started and stream_sender is initialized
 * (reports go out via stream_sender_send()).
 *
 * @return ESP_OK on success.
 */
esp_err_t ftm_ranging_init(void);

/**
 * Start an FTM session to a peer (non-blocking).
 *
 * The result — success or failure — is delivered asynchronously to the
 * aggregator as a 24-byte range report (magic 0xC5110008). Sessions are
 * serialized: if one is already in flight, a status=3 (busy) report is
 * sent immediately for the requested peer.
 *
 * @param peer_mac  Peer MAC address (6 bytes).
 * @return ESP_OK if the session was started, error code otherwise
 *         (a failure report has already been sent in the error cases).
 */
esp_err_t ftm_ranging_start_session(const uint8_t peer_mac[6]);

/**
 * Enable or disable FTM responder mode.
 *
 * Enabling switches WiFi to APSTA and brings up a hidden SoftAP
 * ("RV-FTM-<node_id>", random WPA2 password, ftm_responder=true) on the
 * current STA channel. Disabling returns to plain STA mode.
 *
 * Refused (ESP_ERR_INVALID_STATE) while channel hopping is active — a
 * SoftAP cannot follow the hop table.
 *
 * @param enable  true to enable, false to disable.
 * @return ESP_OK on success.
 */
esp_err_t ftm_ranging_set_responder(bool enable);

#endif /* FTM_RANGING_H */
