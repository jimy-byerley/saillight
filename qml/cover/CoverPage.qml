import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/logic.js" as Logic

// the current folder, what it takes, and its share of the storage it is in
CoverBackground {
    id: cover

    // the ChartModel of the main page
    property var model: pageStack.currentPage ? pageStack.currentPage.model : null
    readonly property bool working: model ? model.scanning || model.busy : false

    // a canvas loses what it drew while it is not shown
    onStatusChanged: if (status === Cover.Active) ring.repaint()

    // a graphic filling the cover behind everything, as the apps of the system have: white
    // shapes whose transparency is in the picture. here a ring chart bigger than the cover
    Image {
        anchors.fill: parent
        sourceSize.width: width
        sourceSize.height: height
        source: Qt.resolvedUrl("../icons/cover.svg")
    }

    // full path of the folder, its last level in the accent color, as on the cover of sailtree.
    // over three lines at most: first levels are left out until it fits
    Label {
        id: path

        readonly property string folder: cover.model ? cover.model.folder : ""
        property int skipped: 0
        function fit() {
            if (truncated && skipped < folder.split("/").length)
                skipped++
        }
        onFolderChanged: skipped = 0
        onTextChanged: fit()
        onTruncatedChanged: fit()

        x: Theme.paddingMedium
        y: Theme.paddingMedium
        width: parent.width - 2 * x
        text: cover.model ? Logic.pathMarkup(folder, Theme.highlightColor, skipped) : "Saillight"
        textFormat: Text.StyledText
        font.pixelSize: Theme.fontSizeExtraSmall
        wrapMode: Text.WrapAnywhere
        maximumLineCount: 3
        elide: Text.ElideRight
    }

    UsageRing {
        id: ring
        anchors {
            top: path.bottom
            topMargin: Theme.paddingMedium
            horizontalCenter: parent.horizontalCenter
        }
        // room is left below for the action
        width: Math.max(0, Math.min(parent.width - 2 * Theme.paddingLarge,
                                    parent.height - y - Theme.itemSizeSmall - Theme.paddingMedium))
        height: width
        folder: cover.model && cover.model.known ? cover.model.totalSize : 0
        used: cover.model ? cover.model.volumeUsed : 0
        total: cover.model ? cover.model.volumeTotal : 0

        Column {
            anchors.centerIn: parent
            width: parent.width - 2 * (ring.thickness + Theme.paddingSmall)

            Label {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                visible: cover.model ? cover.model.known : false
                text: cover.model ? Logic.formatSize(cover.model.totalSize) : ""
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeMedium
                fontSizeMode: Text.HorizontalFit
            }
            Label {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                visible: cover.working || (cover.model ? cover.model.volumeTotal > 0 : false)
                text: {
                    if (!cover.model)
                        return ""
                    if (cover.model.busy)
                        return cover.model.busyText
                    if (cover.model.scanning)
                        return "Scanning"
                    return "of " + Logic.formatSize(cover.model.volumeTotal)
                }
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
                fontSizeMode: Text.HorizontalFit
                truncationMode: TruncationMode.Fade
            }
        }
    }

    CoverActionList {
        enabled: cover.model ? !cover.model.busy : false

        CoverAction {
            iconSource: "image://theme/icon-cover-refresh"
            onTriggered: cover.model.refresh()
        }
    }
}
