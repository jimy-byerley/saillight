.pragma library

// logic of the interface that does not depend on visual items, so it can be unit tested

// human readable file size, eg. "512 B", "1.5 kB", "20 MB"
function formatSize(bytes) {
    var units = ["B", "kB", "MB", "GB", "TB"]
    var value = bytes
    var unit = 0
    while (value >= 1000 && unit < units.length - 1) {
        value /= 1000
        unit += 1
    }
    if (unit === 0)
        return value + " B"
    // one decimal below 10, none above: 1.5 kB, 15 kB
    var text = value < 10 ? (Math.floor(value * 10) / 10).toString() : Math.floor(value).toString()
    return text + " " + units[unit]
}

function pad(number) {
    return number < 10 ? "0" + number : "" + number
}

// local date and time from milliseconds since the epoch, eg. "2026-11-18 20:25"
function formatDate(milliseconds) {
    var date = new Date(milliseconds)
    return date.getFullYear() + "-" + pad(date.getMonth() + 1) + "-" + pad(date.getDate())
            + " " + pad(date.getHours()) + ":" + pad(date.getMinutes())
}

// levels of a path, from the root: [{ name, path }, ...]. the root is always the first one
function crumbs(path) {
    var levels = [{ name: "/", path: "/" }]
    var names = path.split("/")
    var current = ""
    for (var i = 0; i < names.length; i++) {
        if (names[i] === "")
            continue
        current += "/" + names[i]
        levels.push({ name: names[i], path: current })
    }
    return levels
}
