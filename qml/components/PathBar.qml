import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/logic.js" as Logic

// current folder, one clickable label per level, and the warning shown when it changed
// since its sizes were computed
Item {
    id: bar

    property string path
    // the folder changed since its sizes were computed
    property bool stale: false
    // emitted with the path of the clicked level
    signal levelClicked(string path)
    // emitted when the warning is clicked
    signal staleClicked()

    height: Theme.itemSizeSmall

    Flickable {
        id: flick
        anchors {
            fill: parent
            leftMargin: Theme.horizontalPageMargin - Theme.paddingSmall
            rightMargin: warning.visible ? warning.width : Theme.horizontalPageMargin - Theme.paddingSmall
        }
        contentWidth: levels.width
        clip: true
        // a long path shows its end
        onContentWidthChanged: contentX = Math.max(0, contentWidth - width)
        onWidthChanged: contentX = Math.max(0, contentWidth - width)

        Row {
            id: levels
            height: parent.height

            Repeater {
                id: repeater
                model: Logic.crumbs(bar.path)

                BackgroundItem {
                    width: level.width + 2 * Theme.paddingSmall
                    height: levels.height
                    onClicked: bar.levelClicked(modelData.path)

                    Label {
                        id: level
                        anchors.centerIn: parent
                        text: modelData.name === "/" ? "/" : modelData.name + "/"
                        // the last level is the current folder
                        color: parent.highlighted || index === repeater.count - 1
                               ? Theme.highlightColor : Theme.primaryColor
                    }
                }
            }
        }
    }

    IconButton {
        id: warning
        visible: bar.stale
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin - Theme.paddingMedium
            verticalCenter: parent.verticalCenter
        }
        icon.source: "image://theme/icon-m-warning"
        onClicked: bar.staleClicked()
    }
}
