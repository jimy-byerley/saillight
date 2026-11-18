import QtQuick 2.0
import QtTest 1.0
import "../../qml/js/logic.js" as Logic

TestCase {
    name: "logic"

    function test_formatSize_data() {
        return [
            { bytes: 0, text: "0 B" },
            { bytes: 1, text: "1 B" },
            { bytes: 999, text: "999 B" },
            { bytes: 1000, text: "1 kB" },
            { bytes: 1500, text: "1.5 kB" },
            { bytes: 9999, text: "9.9 kB" },
            { bytes: 15300, text: "15 kB" },
            { bytes: 999999, text: "999 kB" },
            { bytes: 1000000, text: "1 MB" },
            { bytes: 2560000, text: "2.5 MB" },
            { bytes: 3000000000, text: "3 GB" },
            { bytes: 7200000000000, text: "7.2 TB" },
            { bytes: 5000000000000000, text: "5000 TB" }
        ]
    }

    function test_formatSize(data) {
        compare(Logic.formatSize(data.bytes), data.text)
    }

    function test_formatDate() {
        // built from local time, so the test does not depend on the timezone
        compare(Logic.formatDate(new Date(2026, 10, 18, 20, 25, 59).getTime()), "2026-11-18 20:25")
        compare(Logic.formatDate(new Date(2001, 0, 2, 3, 4, 5).getTime()), "2001-01-02 03:04")
    }

    function test_crumbs() {
        var crumbs = Logic.crumbs("/home/user/My Documents")
        compare(crumbs.length, 4)
        compare(crumbs[0].name, "/")
        compare(crumbs[0].path, "/")
        compare(crumbs[1].name, "home")
        compare(crumbs[1].path, "/home")
        compare(crumbs[3].name, "My Documents")
        compare(crumbs[3].path, "/home/user/My Documents")

        compare(Logic.crumbs("/").length, 1)
        compare(Logic.crumbs("").length, 1)
        // doubled and trailing separators do not make levels
        crumbs = Logic.crumbs("/srv//files/")
        compare(crumbs.length, 3)
        compare(crumbs[2].path, "/srv/files")
    }

    function test_sectorHsl() {
        var folder = Logic.sectorHsl(0, 1, true)
        var file = Logic.sectorHsl(0, 1, false)
        compare(folder[0], file[0])
        verify(folder[1] > file[1])
        // rings get darker, never black
        verify(Logic.sectorHsl(0, 2, true)[2] < folder[2])
        verify(Logic.sectorHsl(0, 30, true)[2] >= 0.2)
        // all values are valid, and neighbours have different hues
        for (var group = 0; group < 100; group++) {
            var color = Logic.sectorHsl(group, 1, true)
            verify(color[0] >= 0 && color[0] < 1)
            var next = Logic.sectorHsl(group + 1, 1, true)
            var distance = Math.abs(color[0] - next[0])
            verify(Math.min(distance, 1 - distance) > 0.2)
        }
    }

    function test_canvasAngle() {
        fuzzyCompare(Logic.canvasAngle(0), -Math.PI / 2, 1e-9)
        fuzzyCompare(Logic.canvasAngle(0.25), 0, 1e-9)
        fuzzyCompare(Logic.canvasAngle(0.5), Math.PI / 2, 1e-9)
        fuzzyCompare(Logic.canvasAngle(1), 3 * Math.PI / 2, 1e-9)
    }

    function test_archiveExtension() {
        compare(Logic.archiveExtension("zip", true), "zip")
        compare(Logic.archiveExtension("zip", false), "zip")
        compare(Logic.archiveExtension("gz", true), "tar.gz")
        compare(Logic.archiveExtension("gz", false), "gz")
        compare(Logic.archiveExtension("7z", true), "tar.7z")
        compare(Logic.archiveExtension("7z", false), "7z")
    }

    function test_counts() {
        compare(Logic.itemCount(0), "0 items")
        compare(Logic.itemCount(1), "1 item")
        compare(Logic.itemCount(12), "12 items")
        compare(Logic.scanLabel(1500, 3200000000), "1500 items, 3.2 GB")
    }

    function test_cssColor() {
        compare(Logic.cssColor(Qt.rgba(1, 0, 0, 1)), "rgba(255, 0, 0, 1)")
        compare(Logic.cssColor(Qt.rgba(0, 0.5, 1, 1), 0.35), "rgba(0, 128, 255, 0.35)")
        compare(Logic.cssColor(Qt.rgba(0, 0, 0, 0.5)), "rgba(0, 0, 0, 0.5)")
    }
}
