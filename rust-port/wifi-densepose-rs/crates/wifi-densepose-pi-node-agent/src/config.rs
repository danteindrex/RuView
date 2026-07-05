use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub listen_addr: SocketAddr,
    pub aggregator_addr: SocketAddr,
    pub node_base: u8,
    pub default_rssi: i8,
    pub noise_floor: i8,
    pub tier: u8,
    pub enable_mmwave_mock: bool,
    pub enable_wasm: bool,
    pub wasm_path: Option<PathBuf>,
    pub wasm_module_id: u8,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:5500".parse().expect("valid default listen address"),
            aggregator_addr: "127.0.0.1:5005"
                .parse()
                .expect("valid default aggregator address"),
            node_base: 10,
            default_rssi: -55,
            noise_floor: -92,
            tier: 2,
            enable_mmwave_mock: false,
            enable_wasm: false,
            wasm_path: None,
            wasm_module_id: 1,
        }
    }
}

impl AgentConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn from_strings(
        listen_addr: &str,
        aggregator_addr: &str,
        node_base: u8,
        default_rssi: i8,
        noise_floor: i8,
        tier: u8,
        enable_mmwave_mock: bool,
        enable_wasm: bool,
        wasm_path: Option<PathBuf>,
        wasm_module_id: u8,
    ) -> Result<Self> {
        let listen_addr = listen_addr
            .parse::<SocketAddr>()
            .with_context(|| format!("invalid listen address: {listen_addr}"))?;
        let aggregator_addr = aggregator_addr
            .parse::<SocketAddr>()
            .with_context(|| format!("invalid aggregator address: {aggregator_addr}"))?;
        Ok(Self {
            listen_addr,
            aggregator_addr,
            node_base,
            default_rssi,
            noise_floor,
            tier,
            enable_mmwave_mock,
            enable_wasm,
            wasm_path,
            wasm_module_id,
        })
    }

    /// Refuse an aggregator that points at loopback unless explicitly allowed.
    ///
    /// On a Pi node, a loopback aggregator makes the agent stream CSI to the
    /// Pi itself — silently. The aggregator should be the hub PC's LAN IP.
    pub fn ensure_aggregator_not_loopback(&self, allow_loopback: bool) -> Result<()> {
        if self.aggregator_addr.ip().is_loopback() && !allow_loopback {
            anyhow::bail!(
                "aggregator {} is a loopback address: the agent would stream CSI to this machine \
                 itself instead of the hub. Set --aggregator to the hub PC's LAN IP \
                 (e.g. 192.168.1.20:5005), or pass --allow-loopback for local testing.",
                self.aggregator_addr
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_aggregator(aggregator: &str) -> AgentConfig {
        AgentConfig {
            aggregator_addr: aggregator.parse().expect("valid test aggregator address"),
            ..AgentConfig::default()
        }
    }

    #[test]
    fn loopback_aggregator_rejected_by_default() {
        for addr in ["127.0.0.1:5005", "[::1]:5005"] {
            let err = config_with_aggregator(addr)
                .ensure_aggregator_not_loopback(false)
                .expect_err("loopback aggregator must be rejected");
            assert!(err.to_string().contains("--allow-loopback"));
        }
    }

    #[test]
    fn loopback_aggregator_allowed_with_flag() {
        assert!(config_with_aggregator("127.0.0.1:5005")
            .ensure_aggregator_not_loopback(true)
            .is_ok());
    }

    #[test]
    fn lan_aggregator_always_allowed() {
        assert!(config_with_aggregator("192.168.1.20:5005")
            .ensure_aggregator_not_loopback(false)
            .is_ok());
    }
}
