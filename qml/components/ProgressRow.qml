import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/logic.js" as Logic

// progress of the running scan, deletion or compression
Item {
    id: root

    // the ChartModel, or anything with its properties and methods
    property var model
    readonly property bool scanning: model ? model.scanning : false
    readonly property bool busy: model ? model.busy : false
    // a scan shorter than a blink is not announced: the row would only push the chart down
    // and back
    property bool lasting: false
    readonly property bool shown: (scanning && lasting) || busy

    onScanningChanged: {
        lasting = false
        if (scanning)
            patience.restart()
        else
            patience.stop()
    }
    Timer {
        id: patience
        interval: 400
        onTriggered: root.lasting = root.scanning
    }

    height: shown ? Theme.itemSizeMedium : 0
    visible: shown
    clip: true

    ProgressBar {
        id: bar
        anchors {
            left: parent.left
            right: cancel.visible ? cancel.left : parent.right
            verticalCenter: parent.verticalCenter
        }
        minimumValue: 0
        maximumValue: 1
        // a scan cannot tell how far it is, neither can a deletion
        indeterminate: !root.busy || root.model.busyProgress < 0
        value: root.busy && root.model.busyProgress >= 0 ? root.model.busyProgress : 0
        label: {
            if (root.busy)
                return root.model.busyText
            if (root.scanning)
                return "Scanning  ·  " + Logic.scanLabel(root.model.scanItems, root.model.scanBytes)
            return ""
        }
    }

    IconButton {
        id: cancel
        visible: root.busy && root.model.busyCancellable
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin - Theme.paddingMedium
            verticalCenter: parent.verticalCenter
        }
        icon.source: "image://theme/icon-m-clear"
        onClicked: root.model.cancelBusy()
    }
}
