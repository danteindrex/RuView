/**
 * @file discovery_responder.h
 * @brief UDP discovery responder for the Wave desktop console (port 5006).
 *
 * Answers WAVE_DISCOVER broadcasts with a WAVE_BEACON reply and accepts
 * WAVE_HUB re-announcements that re-target the CSI stream at runtime.
 */

#ifndef DISCOVERY_RESPONDER_H
#define DISCOVERY_RESPONDER_H

#include "esp_err.h"

/** UDP port the desktop console broadcasts WAVE_DISCOVER on
 *  (UDP_DISCOVERY_PORT in wifi-densepose-desktop/src/commands/discovery.rs). */
#define DISCOVERY_RESPONDER_PORT 5006

/**
 * Start the discovery responder FreeRTOS task.
 *
 * Binds UDP :5006 and serves these datagram types:
 *  - "WAVE_DISCOVER"        -> reply with
 *    "WAVE_BEACON|<mac>|<node_id>|<version>|<chip>|<role>|<tdm_slot>|<tdm_total>"
 *  - "WAVE_HUB|<ip>|<port>" -> re-target the CSI UDP stream (and persist
 *    the new target to NVS so it survives reboot).
 *  - "WAVE_RANGE|<peer_mac>"       -> start an FTM ranging session
 *    (ADR-091); no ack on :5006, report goes to the aggregator (:5005).
 *  - "WAVE_FTM_RESPONDER|on/off"   -> toggle FTM responder mode
 *    (ADR-091); persisted to NVS key "ftm_resp".
 *
 * Must be called after WiFi is connected and stream_sender is initialized.
 *
 * @return ESP_OK on success, ESP_ERR_NO_MEM if the task could not be created.
 */
esp_err_t discovery_responder_start(void);

#endif /* DISCOVERY_RESPONDER_H */
