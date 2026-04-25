#!/bin/bash

# SHELL SCRIPT TO SETUP NEXMON ON RASPBERRY PI 4B WITH RECENT KERNEL VERSIONS

# 1. Verify your kernel version, as this fix is meant for newer kernels (5.15+)
uname -r

# 1.1 Verify the firmware version your using (should be hiegher that 7_45_189)
dmesg | grep "Firmware: BCM4345"

# 2. Kill wpa_supplicant
sudo pkill wpa_supplicant

# 3. Update your system and install dependencies.
sudo apt update
sudo apt full-upgrade
sudo apt install git libgmp3-dev gawk qpdf bison flex make autoconf libtool texinfo xxd libnl-3-dev libnl-genl-3-dev bc libssl-dev tcpdump

# 4. Do this next step if you are running 64 bit OS
sudo dpkg --add-architecture armhf
sudo apt update
sudo apt-get install libc6:armhf libisl23:armhf libmpfr6:armhf libmpc3:armhf libstdc++6:armhf
sudo ln -s /usr/lib/arm-linux-gnueabihf/libisl.so.23 /usr/lib/arm-linux-gnueabihf/libisl.so.10
sudo ln -s /usr/lib/arm-linux-gnueabihf/libmpfr.so.6 /usr/lib/arm-linux-gnueabihf/libmpfr.so.4

# 5. Install python2.7 as it is required by the bcm43 tool used in this project.
sudo cp /etc/apt/sources.list /tmp/
echo 'deb http://archive.debian.org/debian/ stretch contrib main non-free' | sudo tee -a /etc/apt/sources.list
sudo apt update
sudo apt install python2.7
sudo mv /tmp/sources.list /etc/apt/
sudo apt update

# 6. Fetch and init the nexmon repository.
git clone --depth=1 https://github.com/seemoo-lab/nexmon.git
cd nexmon
source setup_env.sh
sed -i '1 s/$/2.7/' $NEXMON_ROOT/buildtools/b43-v3/debug/b43-beautifier
make # NOTE: This will display a number of warnings, as long as it complete's without actual error messages, it should be fine. If you get an error about "arm-none-eabi-gcc: not found", ensure you have executed step 5 and that the armhf architecture is properly configured on your system.

# 7. Build and install nexutil
cd $NEXMON_ROOT/utilities/nexutil
sudo -E make install USE_VENDOR_CMD=1
sudo setcap cap_net_admin+ep /usr/bin/nexutil

# 8. Fetch the nexmon_csi repository
cd $NEXMON_ROOT/patches/bcm43455c0/7_45_189 # it must be executed from this directory, as the scripts in the next step are built for this version. 
git clone --depth=1 https://github.com/seemoo-lab/nexmon_csi.git
cd nexmon_csi

# 9. Install the nexmon_csi firmware
make -f Makefile.rpi install-firmware

# 9.1 If the above returns the following error "Makefile.rpi:2: *** recipe commences before first target.  Stop.", try sourcing setup_env.sh again and then running the command again
source $NEXMON_ROOT/setup_env.sh
make -f Makefile.rpi install-firmware

# 9.2 If after installing you get the following error, ensure you executed step 5.
  COMPILING src/console.c => obj/console.o (details: log/compiler.log)
/bin/sh: 1: /home/pi/nexmon/buildtools/gcc-arm-none-eabi-5_4-2016q2-linux-armv7l/bin/arm-none-eabi-gcc: not found
make: *** [Makefile.rpi:76: obj/console.o] Fehler 127

# 10. Resume the remainder of the patch
# NOTE: Running unmanage will take the wifi interface down. Thus, if you are connected to your pi via wifi and there are no other SSIDs it can connect to, conect it via ethenet, otherwise you will lose access to your pi unless you connect peripherals to it. 
make -f Makefile.rpi unmanage 
make -f Makefile.rpi reload-full

# 13. Go to makecsiparams in nexmon_csi utils to generate and copy the config string you'll need for the next step.
cd utils/makecsiparams
make
./makecsiparams -c 36/80 -C 1 -N 1 # or whatever channel and bandwidth you want to use, it should output something like this and close, "KuABEQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="
cd ../..

# 14. Configure the CSI extractor and activate monitor mode
nexutil -s500 -b -l34 -v<your-config-generated-with-makecsiparams>
nexutil -m1

# 16. Demo capturing CSI UDPs using tcpdump
sudo tcpdump -i wlan0 dst port 5500

# NOTE: To reset the firmware to its default, and give control back to network manager, run this:
make -f Makefile.rpi restore-wifi

# 17. Reboot the system
sudo reboot

# 18. After reboot, the wifi interface should be back up and working, but it will still be unmanaged. 
# This means that you won't be able to connect to wifi networks using network manager, but you can still use the interface for monitor mode and CSI extraction. You can verify this by running:
nmcli device status # this should show that wlan0 is unmanaged
dmesg | grep "Firmware: BCM4345" # you'll notice that the firmware version is now 7_45_189. This is expected as it does the firmware swap for us

# 19. In order to execute step 16 again, unblock from rfkill and get the interface up (it will still show that its down if you run sudo ip link show wlan0)
rfkill list # confirm that indeed wlan0 is blocked
sudo rfkill unblock all # this ensures that you can bring the wlan0 interface back up
sudo ip link set wlan0 up

# 20. Repeat steps 14 and 16 to start streaming CSI on the terminal again.

# TODOs:
# - A shell script that auto starts CSI capture and sstreaming on device startup, and another one to stop it and restore the firmware to its default state. 

