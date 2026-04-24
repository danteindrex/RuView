#!/bin/bash

# SHELL SCRIPT TO SETUP NEXMON ON RASPBERRY PI 4B WITH RECENT KERNEL VERSIONS

# 1. Verify the firmware version your using, the latest nexmon supports is 7_45_241.
dmesg | grep "Firmware: BCM4345"

# 2. If your firware verion is more recent than 7_45_241, follow these steps to downgrade. Otherwise, skip to step 3.
# 2.1 Download the older firmware version from the official repository and unpack it. You can find it here:
cd ../..
wget https://archive.raspberrypi.com/debian/pool/main/f/firmware-nonfree/firmware-brcm80211_20221012-1~bpo11+1+rpt1_all.deb 
dpkg-deb -x firmware-brcm80211_20221012-1~bpo11+1+rpt1_all.deb /tmp/fw-extract

# 2.2 Backup the current firmware just incase you need to restore it later
sudo cp /lib/firmware/brcm/brcmfmac43455-sdio.bin \
        /lib/firmware/brcm/brcmfmac43455-sdio.bin.backup-265 # 265 cause that's the latest firmware version as of the making of this script
sudo cp /lib/firmware/brcm/brcmfmac43455-sdio.clm_blob \
        /lib/firmware/brcm/brcmfmac43455-sdio.clm_blob.backup-265 2>/dev/null
sudo cp /lib/firmware/brcm/brcmfmac43455-sdio.txt \
        /lib/firmware/brcm/brcmfmac43455-sdio.txt.backup-265 2>/dev/null

# 2.3.1 Install the older firmware (downgrading)
sudo dpkg -i --force-downgrade firmware-brcm80211_20221012-1~bpo11+1+rpt1_all.deb

# 2.3.2 Alternatively, you can manually copy the older firmware files to the appropriate location:
sudo cp /tmp/fw-extract/lib/firmware/brcm/brcmfmac43455-sdio_7.45.241.bin \
        /lib/firmware/brcm/brcmfmac43455-sdio.bin

# 2.4 Prevent apt from upgrading it later
sudo apt-mark hold firmware-brcm80211

# 2.5 Reboot the system to apply the changes
sudo reboot

# 2.6 You can check the firmware version again as stated in step 1 to confirm the downgrade was successful.

# 3. Return back to the home directory, and kill wpa_supplicant
cd ~
sudo pkill wpa_supplicant

# 4. Update your system and install dependencies.
sudo apt update
sudo apt full-upgrade
sudo apt install git libgmp3-dev gawk qpdf bison flex make autoconf libtool texinfo xxd libnl-3-dev libnl-genl-3-dev bc libssl-dev tcpdump

# 5. Do this next step if you are running 64 bit OS, if not, skip to step 6.
sudo dpkg --add-architecture armhf
sudo apt update
sudo apt-get install libc6:armhf libisl23:armhf libmpfr6:armhf libmpc3:armhf libstdc++6:armhf
sudo ln -s /usr/lib/arm-linux-gnueabihf/libisl.so.23 /usr/lib/arm-linux-gnueabihf/libisl.so.10
sudo ln -s /usr/lib/arm-linux-gnueabihf/libmpfr.so.6 /usr/lib/arm-linux-gnueabihf/libmpfr.so.4

# 6. Install python2.7 as it is required by the bcm43 tool used in this project.
sudo cp /etc/apt/sources.list /tmp/
echo 'deb http://archive.debian.org/debian/ stretch contrib main non-free' | sudo tee -a /etc/apt/sources.list
sudo apt update
sudo apt install python2.7
sudo mv /tmp/sources.list /etc/apt/
sudo apt update

# 7. Fetch and init the nexmon repository.
git clone --depth=1 https://github.com/seemoo-lab/nexmon.git
cd nexmon
source setup_env.sh
sed -i '1 s/$/2.7/' $NEXMON_ROOT/buildtools/b43-v3/debug/b43-beautifier
make

# 8. Build and install nexutil
$ cd $NEXMON_ROOT/utilities/nexutil
$ sudo -E make install USE_VENDOR_CMD=1
$ sudo setcap cap_net_admin+ep /usr/bin/nexutil

# 9. Fetch the nexmon_csi repository
$ cd $NEXMON_ROOT/patches/bcm43455c0/7_45_241 # navigate to the appropriate directory for your firmware version
$ git clone --depth=1 https://github.com/seemoo-lab/nexmon_csi.git
$ cd nexmon_csi

# 10. Install the nexmon_csi firmware
make -f Makefile.rpi install-firmware

# 10.1 If the above returns the following error "Makefile.rpi:2: *** recipe commences before first target.  Stop.", try sourcing setup_env.sh again and then running the command again
source $NEXMON_ROOT/setup_env.sh
make -f Makefile.rpi install-firmware

# 10.2 If after installing you get the following error, ensure you executed step 5.
  COMPILING src/console.c => obj/console.o (details: log/compiler.log)
/bin/sh: 1: /home/pi/nexmon/buildtools/gcc-arm-none-eabi-5_4-2016q2-linux-armv7l/bin/arm-none-eabi-gcc: not found
make: *** [Makefile.rpi:76: obj/console.o] Fehler 127

# 11. Resume the remainder of the patch
# NOTE: Running unmanage will take the wifi interface down. Thus, if you are connected to your pi via wifi and there are no other SSIDs it can connect to, conect it via ethenet, otherwise you will lose access to your pi unless you connect peripherals to it. 
make -f Makefile.rpi unmanage 
make -f Makefile.rpi reload-full

# 12. Manually pull the wifi interface up after unmanaging using nexmon, then confirm it is up and unmanaged
sudo ip link set wlan0 up
nmcli device status
ip link show wlan0
ip a # should it remain down, ensure all steps prior to this went well 

# 13. Go to makecsiparams in nexmon_csi utils to generate and copy the config string you'll need for the next step.
cd utils/makecsiparams
make
./makecsiparams -c 36/80 -C 1 -N 1 # or whatever channel and bandwidth you want to use

# 14. Configure the CSI extractor and activate monitor mode
cd ../..
nexutil -s500 -b -l34 -v<your-config-generated-with-makecsiparams>
nexutil -m1

# 15. Confirm nexmon is working
sudo nexutil -Iwlan0 -s

# 16. Demo capturing CSI UDPs using tcpdump
sudo tcpdump -i wlan0 dst port 5500 -vv -w ~/csi-capture.pcap

# NOTE: To reset the firmware to its default, and give control back to network manager, run this:
make -f Makefile.rpi restore-wifi


