import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/logic.js" as Logic

// the current folder and what it takes
CoverBackground {
    id: cover

    // the ChartModel of the main page
    property var model: pageStack.currentPage ? pageStack.currentPage.model : null

    Column {
        anchors.centerIn: parent
        width: parent.width - 2 * Theme.paddingLarge
        spacing: Theme.paddingSmall

        Label {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: cover.model ? cover.model.folderName : "Saillight"
            truncationMode: TruncationMode.Fade
        }
        Label {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            visible: cover.model ? cover.model.known : false
            text: cover.model ? Logic.formatSize(cover.model.totalSize) : ""
            color: Theme.highlightColor
            font.pixelSize: Theme.fontSizeLarge
        }
        Label {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            visible: cover.model ? cover.model.scanning || cover.model.busy : false
            text: cover.model && cover.model.busy ? cover.model.busyText : "Scanning"
            color: Theme.secondaryColor
            font.pixelSize: Theme.fontSizeExtraSmall
            wrapMode: Text.Wrap
        }
    }
}
