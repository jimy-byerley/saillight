#!/bin/bash
# drive the installed app in the SDK emulator without touches, which cannot be simulated there:
# the main page gets a timer running one javascript statement every 3 seconds, and a
# screenshot is taken after each
#   scripts/emulator-drive.sh <folder for screenshots> "<statement>" ["<statement>" ...]
# statements see the ids of qml/pages/MainPage.qml, eg.
#   scripts/emulator-drive.sh target/drive "chartModel.select(0)" "chartModel.enterSelected()"
# the app must be installed (scripts/emulator-run.sh). the qml of the project is copied first,
# so qml changes need no rebuild. the real main page is put back at the end.
# only ever talks to the emulator (ssh on 127.0.0.1:2223), never to a real device
set -e
cd "$(dirname "$0")/.."
SDK=${SDK:-$HOME/jolla/sdk}
SSH_OPTIONS="-i $SDK/vmshare/ssh/private_keys/sdk -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o BatchMode=yes"
user() { ssh -p 2223 $SSH_OPTIONS defaultuser@127.0.0.1 "$@"; }
SESSION='export XDG_RUNTIME_DIR=/run/user/$(id -u) WAYLAND_DISPLAY=../../display/wayland-0; export DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/dbus/user_bus_socket;'

OUT=$1
shift
[ -n "$OUT" ] && [ $# -gt 0 ] || { sed -n '2,10p' "$0"; exit 1; }
mkdir -p "$OUT"

STEPS=""
for statement in "$@"; do STEPS="$STEPS function() { $statement },"; done
DRIVEN=$(mktemp)
trap 'rm -f "$DRIVEN"' EXIT
python3 - "$STEPS" > "$DRIVEN" <<'EOF'
import sys
page = open('qml/pages/MainPage.qml').read()
timer = '''
    Timer {
        property int step: 0
        property var steps: [%s]
        interval: 3000; repeat: true; running: true
        onTriggered: if (step < steps.length) { console.log("drive step", step); steps[step++]() }
    }
}''' % sys.argv[1]
print(page[:page.rindex('}')] + timer)
EOF

scp -q -r -P 2223 $SSH_OPTIONS qml root@127.0.0.1:/usr/share/saillight/
scp -q -P 2223 $SSH_OPTIONS "$DRIVEN" root@127.0.0.1:/usr/share/saillight/qml/pages/MainPage.qml
# the screenshot service only answers to the privileged group
ssh -p 2223 $SSH_OPTIONS root@127.0.0.1 'id -Gn defaultuser | grep -qw privileged || usermod -a -G privileged defaultuser'
user "$SESSION"' pkill -x saillight; sleep 0.5; QT_LOGGING_TO_CONSOLE=1 setsid saillight > /tmp/saillight.log 2>&1 < /dev/null &'

for ((step = 0; step < $#; step++)); do
    for ((wait = 0; wait < 100; wait++)); do
        user "grep -q 'drive step $step\$' /tmp/saillight.log" && break
        sleep 0.2
    done
    sleep 1.2
    user "$SESSION"' rm -f shot.png; sg privileged -c "dbus-send --session --print-reply --dest=org.nemomobile.lipstick /org/nemomobile/lipstick/screenshot org.nemomobile.lipstick.saveScreenshot string:$HOME/shot.png" > /dev/null; sleep 0.7; cat shot.png; rm -f shot.png' > "$OUT/step$step.png"
done

echo "--- app output"
user 'pkill -x saillight; cat /tmp/saillight.log'
scp -q -P 2223 $SSH_OPTIONS qml/pages/MainPage.qml root@127.0.0.1:/usr/share/saillight/qml/pages/MainPage.qml
echo "screenshots saved to $OUT"
