import QtQuick 2.0
import Sailfish.Silica 1.0

// an action on the selected item: an icon of the theme or of the app (qml/icons), with a short text below when the
// icon alone does not tell everything
MouseArea {
    id: root

    // what the action is, for whoever looks for it
    property string action
    // name of an icon of the theme, eg. "icon-m-delete", or of the app, eg. "enter"
    property string icon
    property string caption
    readonly property bool down: pressed && containsMouse

    height: Math.max(Theme.itemSizeSmall, content.height + Theme.paddingSmall)
    opacity: enabled ? 1 : Theme.opacityLow

    Column {
        id: content
        width: parent.width
        anchors.top: parent.top

        HighlightImage {
            anchors.horizontalCenter: parent.horizontalCenter
            width: Theme.iconSizeMedium
            height: Theme.iconSizeMedium
            sourceSize.width: width
            sourceSize.height: height
            source: root.icon.indexOf("icon-") === 0 ? "image://theme/" + root.icon
                                                     : Qt.resolvedUrl("../icons/" + root.icon + ".svg")
            highlighted: root.down
        }
        Label {
            visible: root.caption !== ""
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: root.caption
            color: root.down ? Theme.highlightColor : Theme.secondaryColor
            font.pixelSize: Theme.fontSizeExtraSmall
            truncationMode: TruncationMode.Fade
        }
    }
}
