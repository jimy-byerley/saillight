import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/logic.js" as Logic

// current folder, one clickable label per level, and the warning shown when it changed
// since its sizes were computed
Item {
    id: bar

    property string path
    // emitted with the path of the clicked level
    signal levelClicked(string path)

    height: Theme.itemSizeSmall

    Flickable {
        anchors.fill: parent
        anchors.leftMargin: Theme.horizontalPageMargin
        anchors.rightMargin: Theme.horizontalPageMargin
        contentWidth: levels.width
        clip: true
        // a long path shows its end
        onContentWidthChanged: contentX = Math.max(0, contentWidth - width)

        Row {
            id: levels
            height: parent.height

            Repeater {
                model: Logic.crumbs(bar.path)

                BackgroundItem {
                    width: level.width + Theme.paddingMedium
                    height: levels.height
                    onClicked: bar.levelClicked(modelData.path)

                    Label {
                        id: level
                        anchors.centerIn: parent
                        text: modelData.name === "/" ? "/" : modelData.name + "/"
                    }
                }
            }
        }
    }
}
