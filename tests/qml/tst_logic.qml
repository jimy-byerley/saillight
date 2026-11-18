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
}
