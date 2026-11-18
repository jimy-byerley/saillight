import QtQuick 2.0
import Sailfish.Silica 1.0

// draws the sectors computed by the model, and reports press, slide and release.
// no geometry is computed here
Item {
    id: chart

    Canvas {
        id: sectors
        anchors.fill: parent
    }
    // the hovered sector is drawn apart, so sliding does not repaint the whole chart
    Canvas {
        id: hover
        anchors.fill: parent
    }
    MouseArea {
        anchors.fill: parent
    }
}
