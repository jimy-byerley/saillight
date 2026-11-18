import QtQuick 2.6
import Sailfish.Silica 1.0
import "../js/logic.js" as Logic

// the selected sector: path, properties, set as current folder, delete, compress
Column {
    id: root

    // the ChartModel, or anything with its properties and methods
    property var model
    property var details: model ? model.details : ({})
    readonly property bool shown: model ? model.selected >= 0 : false
    readonly property bool marker: details.kind === "unreadable" || details.kind === "mount"
    readonly property bool allowed: shown && !marker && details.refusal === ""
                                    && !model.busy && !model.scanning

    // the user asked to delete the item at this path: the page asks for confirmation
    signal deleteRequested(string path, string name)

    visible: shown
    spacing: Theme.paddingSmall
    leftPadding: Theme.horizontalPageMargin
    rightPadding: Theme.horizontalPageMargin
    readonly property real innerWidth: width - leftPadding - rightPadding

    Label {
        width: root.innerWidth
        text: root.details.name || ""
        color: Theme.highlightColor
        font.pixelSize: Theme.fontSizeLarge
        truncationMode: TruncationMode.Fade
    }

    Label {
        width: root.innerWidth
        text: root.details.path || ""
        color: Theme.secondaryColor
        font.pixelSize: Theme.fontSizeExtraSmall
        wrapMode: Text.WrapAnywhere
    }

    // a file or folder
    Label {
        visible: root.shown && !root.marker
        width: root.innerWidth
        font.pixelSize: Theme.fontSizeSmall
        wrapMode: Text.Wrap
        text: {
            if (!visible)
                return ""
            var parts = [Logic.formatSize(root.details.size)]
            if (root.details.kind === "folder")
                parts.push(Logic.itemCount(root.details.count))
            else if (root.details.kind === "link")
                parts.push("link")
            parts.push(Logic.formatDate(root.details.date))
            return parts.join("  ·  ")
        }
    }

    Label {
        visible: root.shown && !root.marker
        width: root.innerWidth
        color: Theme.secondaryColor
        font.pixelSize: Theme.fontSizeSmall
        text: visible ? root.details.permissions + "  " + root.details.owner : ""
    }

    // a marker: what its folder holds that is not counted
    Label {
        visible: root.marker && root.details.blocked.length > 0
        width: root.innerWidth
        font.pixelSize: Theme.fontSizeSmall
        wrapMode: Text.Wrap
        text: "Not counted, as these could not be read:"
    }

    Repeater {
        model: root.marker ? root.details.blocked : []

        Label {
            width: root.innerWidth
            text: modelData
            color: Theme.secondaryColor
            font.pixelSize: Theme.fontSizeExtraSmall
            wrapMode: Text.WrapAnywhere
        }
    }

    Label {
        visible: root.marker && root.details.mounts.length > 0
        width: root.innerWidth
        font.pixelSize: Theme.fontSizeSmall
        wrapMode: Text.Wrap
        text: "Not counted, as these are other storages. Tap one to open it:"
    }

    Repeater {
        model: root.marker ? root.details.mounts : []

        BackgroundItem {
            x: -root.leftPadding
            width: root.width
            height: Math.max(Theme.itemSizeExtraSmall, mountPath.implicitHeight + Theme.paddingMedium)
            onClicked: root.model.setFolder(modelData)

            Label {
                id: mountPath
                x: root.leftPadding
                width: root.innerWidth
                anchors.verticalCenter: parent.verticalCenter
                text: modelData
                color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
                wrapMode: Text.WrapAnywhere
            }
        }
    }

    Label {
        visible: root.shown && !root.marker && text !== ""
        width: root.innerWidth
        text: root.details.refusal || ""
        color: Theme.secondaryHighlightColor
        font.pixelSize: Theme.fontSizeExtraSmall
        wrapMode: Text.Wrap
    }

    Item { width: 1; height: Theme.paddingSmall }

    Row {
        visible: root.shown && !root.marker
        spacing: Theme.paddingMedium

        Button {
            visible: root.details.enterable === true
            width: (root.innerWidth - Theme.paddingMedium) / 2
            text: "Open"
            enabled: root.model ? !root.model.busy : false
            onClicked: root.model.enterSelected()
        }
        Button {
            width: (root.innerWidth - Theme.paddingMedium) / 2
            text: "Delete"
            enabled: root.allowed
            onClicked: root.deleteRequested(root.details.path, root.details.name)
        }
    }

    Label {
        visible: compressRow.visible
        width: root.innerWidth
        text: "Compress as"
        color: Theme.secondaryColor
        font.pixelSize: Theme.fontSizeExtraSmall
    }

    Row {
        id: compressRow
        // only files and folders can be compressed
        visible: root.shown && (root.details.kind === "file" || root.details.kind === "folder")
        spacing: Theme.paddingMedium

        Repeater {
            model: ["zip", "gz", "7z"]

            Button {
                width: (root.innerWidth - 2 * Theme.paddingMedium) / 3
                text: Logic.archiveExtension(modelData, root.details.kind === "folder")
                enabled: root.allowed
                onClicked: root.model.compressItem(root.details.path, modelData)
            }
        }
    }

    Item { width: 1; height: Theme.paddingLarge }
}
