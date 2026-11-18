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

// color of a sector, as [hue, saturation, lightness] between 0 and 1.
// the sectors of a same first-ring item share a hue, rings get darker outward,
// and files are less saturated than folders
function sectorHsl(group, ring, folder) {
    // hues spread by the golden ratio, so neighbours differ whatever their number
    var hue = (0.58 + group * 0.618033988749895) % 1
    var saturation = folder ? 0.62 : 0.38
    var lightness = Math.max(0.24, 0.58 - (ring - 1) * 0.07)
    return [hue, saturation, lightness]
}

// a color as text for a canvas, with another opacity. canvases are only given colors as text:
// with Qt 5.6 a color given as a value is drawn, but the next color given as text may be ignored
function cssColor(color, opacity) {
    return "rgba(" + Math.round(color.r * 255) + ", " + Math.round(color.g * 255) + ", "
            + Math.round(color.b * 255) + ", "
            + Math.round((opacity === undefined ? color.a : opacity) * 1000) / 1000 + ")"
}

// angle in radians, for a canvas, of an angle in turns from the top
function canvasAngle(turn) {
    return turn * 2 * Math.PI - Math.PI / 2
}

// what compressing adds to a name: folders go through tar, except for zip
function archiveExtension(format, folder) {
    if (format === "zip")
        return "zip"
    return (folder ? "tar." : "") + format
}

// "1 item", "12 items"
function itemCount(count) {
    return count === 1 ? "1 item" : count + " items"
}

// what a scan in progress shows: "1 234 items, 3.2 GB"
function scanLabel(items, bytes) {
    return itemCount(items) + ", " + formatSize(bytes)
}
