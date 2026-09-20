# dmgbuild settings for the Mac2Win Go installer.
#
# Invoked by scripts/build-dmg.sh with -D overrides:
#   app_path        absolute path to "Mac2Win Go.app"
#   app_name        file name inside the volume (e.g. "Mac2Win Go.app")
#   background      absolute path to assets/dmg_background.png
#   icon            absolute path to assets/AppIcon.icns
#   instructions    absolute path to assets/How to open.txt (optional)
#
# dmgbuild writes .DS_Store directly, so the window layout works on headless
# CI runners where Finder/AppleScript is unavailable.

app_path = defines["app_path"]
app_name = defines["app_name"]
background = defines["background"]
icon = defines.get("icon")
instructions = defines.get("instructions")

files = [(app_path, app_name)]
symlinks = {"Applications": "/Applications"}

icon_locations = {
    app_name: (165, 210),
    "Applications": (495, 210),
}

if instructions:
    instructions_name = "How to open.txt"
    files.append((instructions, instructions_name))
    # Centred below the app and Applications icons. The two icon labels end
    # around y=310, the arrow drawn between them spans y=165..255, and this
    # icon (128pt tall, centred at y=325) sits clear of both.
    icon_locations[instructions_name] = (330, 325)

window_rect = ((200, 200), (660, 420))
default_view = "icon-view"
icon_size = 128
text_size = 13

show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
include_icon_view_settings = True

hide_extensions = [app_name]
format = "UDZO"
