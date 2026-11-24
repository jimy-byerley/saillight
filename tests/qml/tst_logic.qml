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

    function test_hsl() {
        var red = Logic.hsl(Qt.rgba(1, 0, 0, 1))
        fuzzyCompare(red[0], 0, 1e-9)
        fuzzyCompare(red[1], 1, 1e-9)
        fuzzyCompare(red[2], 0.5, 1e-9)
        fuzzyCompare(Logic.hsl(Qt.rgba(0, 1, 0, 1))[0], 1 / 3, 1e-9)
        fuzzyCompare(Logic.hsl(Qt.rgba(0, 0, 1, 1))[0], 2 / 3, 1e-9)
        // magenta side of red stays below a turn
        var pink = Logic.hsl(Qt.rgba(1, 0, 0.5, 1))
        verify(pink[0] > 0.9 && pink[0] < 1)
        // greys have no saturation
        compare(Logic.hsl(Qt.rgba(0.5, 0.5, 0.5, 1))[1], 0)
        compare(Logic.hsl(Qt.rgba(1, 1, 1, 1))[1], 0)
        compare(Logic.hsl(Qt.rgba(0, 0, 0, 1))[2], 0)
        // back to the same color
        var color = Qt.rgba(0.2, 0.6, 0.9, 1)
        var parts = Logic.hsl(color)
        var back = Qt.hsla(parts[0], parts[1], parts[2], 1)
        fuzzyCompare(back.r, color.r, 0.01)
        fuzzyCompare(back.g, color.g, 0.01)
        fuzzyCompare(back.b, color.b, 0.01)
    }

    function test_sectorHsla() {
        var base = [0.55, 0.8, 0.6]
        var folder = Logic.sectorHsla(base, 0, 1, true)
        var file = Logic.sectorHsla(base, 0, 1, false)
        // the first item has the hue of the theme
        compare(folder[0], base[0])
        compare(folder[0], file[0])
        verify(folder[1] > file[1])
        // rings fade outward, never away
        verify(Logic.sectorHsla(base, 0, 2, true)[3] < folder[3])
        verify(Logic.sectorHsla(base, 0, 30, true)[3] >= 0.2)
        // all values are valid whatever the theme, hues stay close to its own,
        // and neighbours differ
        var bases = [base, [0.02, 1, 0.9], [0.98, 0, 0.1]]
        for (var index = 0; index < bases.length; index++) {
            for (var group = 0; group < 100; group++) {
                var color = Logic.sectorHsla(bases[index], group, 1 + group % 6, group % 2 === 0)
                for (var part = 0; part < 4; part++)
                    verify(color[part] >= 0 && color[part] <= 1)
                verify(color[0] < 1)
                var distance = Math.abs(color[0] - bases[index][0])
                verify(Math.min(distance, 1 - distance) < 0.1)
                var next = Logic.sectorHsla(bases[index], group + 1, 1, true)
                var same = Logic.sectorHsla(bases[index], group, 1, true)
                verify(next[0] !== same[0] || next[2] !== same[2])
            }
        }
    }

    function test_fraction() {
        compare(Logic.fraction(1, 4), 0.25)
        compare(Logic.fraction(5, 4), 1)
        compare(Logic.fraction(-1, 4), 0)
        compare(Logic.fraction(1, 0), 0)
        compare(Logic.usageLabel(1500, 20000000), "1.5 kB of 20 MB")
    }

    function test_volumeShares() {
        var shares = Logic.volumeShares(100, 400, 1000)
        fuzzyCompare(shares[0], 0.1, 1e-9)
        fuzzyCompare(shares[1], 0.3, 1e-9)
        fuzzyCompare(shares[2], 0.6, 1e-9)
        // a folder counted bigger than what is used does not go over it
        shares = Logic.volumeShares(900, 400, 1000)
        fuzzyCompare(shares[0], 0.4, 1e-9)
        fuzzyCompare(shares[1], 0, 1e-9)
        fuzzyCompare(shares[0] + shares[1] + shares[2], 1, 1e-9)
        compare(Logic.volumeShares(100, 400, 0), [0, 0, 1])
        compare(Logic.volumeShares(0, 0, 1000), [0, 0, 1])
    }

    function test_pathMarkup() {
        compare(Logic.pathMarkup("/home/user/Documents", "#f00", 0), '/home/user/<font color="#f00">Documents</font>')
        compare(Logic.pathMarkup("/home/user/", "#f00", 0), '/home/<font color="#f00">user</font>')
        compare(Logic.pathMarkup("/etc", "#f00", 0), '/<font color="#f00">etc</font>')
        compare(Logic.pathMarkup("/", "#f00", 0), '<font color="#f00">/</font>')
        compare(Logic.pathMarkup("/a<b/c&d", "#f00", 0), '/a&lt;b/<font color="#f00">c&amp;d</font>')
        // first levels left out
        compare(Logic.pathMarkup("/home/user/Documents", "#f00", 1), '\u2026/user/<font color="#f00">Documents</font>')
        compare(Logic.pathMarkup("/home/user/Documents", "#f00", 2), '\u2026/<font color="#f00">Documents</font>')
        compare(Logic.pathMarkup("/home/user/Documents", "#f00", 9), '\u2026/<font color="#f00">Documents</font>')
        compare(Logic.pathMarkup("/", "#f00", 3), '<font color="#f00">/</font>')
    }

    function test_canvasAngle() {
        fuzzyCompare(Logic.canvasAngle(0), -Math.PI / 2, 1e-9)
        fuzzyCompare(Logic.canvasAngle(0.25), 0, 1e-9)
        fuzzyCompare(Logic.canvasAngle(0.5), Math.PI / 2, 1e-9)
        fuzzyCompare(Logic.canvasAngle(1), 3 * Math.PI / 2, 1e-9)
    }

    function test_fileUrl() {
        compare(Logic.fileUrl("/home/user/notes.txt"), "file:///home/user/notes.txt")
        compare(Logic.fileUrl("/home/user/my file #1?.txt"), "file:///home/user/my%20file%20%231%3F.txt")
        compare(Logic.fileUrl("/"), "file:///")
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
