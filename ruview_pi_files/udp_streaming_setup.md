## SETTING UP UDP PIPELINE TO STREAM CSI TO A REMOTE DEVICE

> **NOTE:**
> In order for these to be successful, nexmon must be up and in monitor mode. To do this, refer to instructions 16-17 in nexmon_setup.md
> These instructions assume your aggregator is running on a machine with windows.
> I advise using tailscale to create a private network where each device has a static IP, which makes commnication, ssh and debugging easier.

If you want to capture CSI to a pcap file:

```bash
sudo tcpdump -i wlan0 dst port 5500 -vv -w ~/path/to/save/csi.pcap
```

## 1. First verify that the Pi can send UDP to the aggregator/laptop by using netcat (ncat on windows)

```bash
winget install Nmap
ncat -ul 5005
```

On the Pi:

```bash
echo "test from pi" | nc -u -w1 <laptop-tailnet-ip> 5005
```

> If that message appears on the windows terminal, proceed. If not, troubleshoot any network issues preventing the communication

## 2. Install the rust toolchain to build the agents required for the next steps

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustc --version
cargo --version
```

## 3. Clone this repository onto the raspberry pi, as it contains the agent node responsible for converting pcap CSI packets to the ADR-018 format required by RuView

```bash
git clone https://github.com/danteindrex/RuView.git
```

## 4. Build the Pi node agent

```bash
cd RuView/rust-port/wifi-densepose-rs
cargo build --release -p wifi-densepose-pi-node-agent
```

## 5. Confirm the agent was successfully built by looking at available arguements and help menu (output of compile goes to ./target/release/)

```bash
./target/release/wifi-densepose-pi-node-agent --help
```

## 6. Start UDP bridge to forward CSI packets from the monitor mode interface to the agent

It is important to know that at present the agent isn't capable of listening to CSI packets broadcasted to 255.255.255.255:5500, like tcpdump can.
Thus a helper bridge script must be run to pick CSI packets from the interface, remove unecessary headers and feed the resultant packets to the agent.
The script for this is located at the same directory as this script (nexmon_bridge.py)

```bash
cd ~
cp RuView/ruview_pi_files/nexmon_bridge.py scripts/nexmon_bridge.py
sudo tcpdump -i wlan0 -U -w - 'dst port 5500' | python scripts/nexmon_bridge.py
```

## 7. Run the agent pointing at your aggregator

```bash
cd RuView/rust-port/wifi-densepose-rs
RUST_LOG=debug ./target/release/wifi-densepose-pi-node-agent --listen 127.0.0.1:5501 --aggregator 100.113.88.28:5005 --node-base 1
```

## 8. Verify the CSI data is being processed and sent to the aggregator (on the Pi)

```bash
sudo tcpdump -i any host <aggregator-tailnet-ip> and port 5005
```

## 9. Validate that the CSI data is being received by the aggregator, if the machine is windows.

```bash
ncat -ul 5005
```

> assuming that you installed Nmap

## 10. Force sensing mode on the aggregator/machine

```bash
docker pull ruvnet/wifi-densepose:latest
docker run -p 3000:3000 -p 3001:3001 -p 5005:5005/udp -e CSI_SOURCE=esp32 --name ruview ruvnet/wifi-densepose:latest
```
