# VOR
### Beta: many bugs still, as well as high performance only (Will use about 25-30% CPU depending on how many apps).
### Working on adding an optimized mode, as the high CPU usage is intended for performance for my personal use case.
### Please report bugs!
- Note that VOR is meant to be used to route OSC traffic that is RECEIVED from VRChat. All OSC apps can send to VRC on their own, but VRChat can only send to one port.

CLI Args:
--enable-on-start / -e

# Install

1. Download the [latest](https://github.com/SutekhVRC/VOR/releases/latest) MSI and run it.
2. vor.exe will be installed to C:\Program Files\vor\bin\vor.exe. You can also search for it by pressing the windows key and searching "VOR".

# Setup an app

1. Once VOR is opened go to the "Apps" tab. This is where you can add "apps". An app is basically a route to send to.

2. To add an app click the plus to the right of "Add new VOR app"

3. 
    - App Name(must be unique from other apps): Put an app identifier here VibeCheck/RemiOSC/etc.
    - App Host: This is the host that VOR will route the traffic FROM VRChat TO your app (Whatever host your app is listening on).
    - App Port: The port your app is listening on.
    - Bind Host: The host/interface to bind the route UDP socket to (This will probably ALWAYS be 127.0.0.1).
    - Bind Port: The port to bind the route UDP socket to. This can be any port that is not being used anywhere else. These must be unique between every VOR app/route you add. NOTE: It is best to keep your bind ports in the higher range to reduce the likelihood of it interfering with another service.
    - Click Add

4. Remember to set your OSC apps to bind on different ports (The "App Ports" in VOR). And they should still be sending directly to VRChat (VRChat default bind port is 9000).