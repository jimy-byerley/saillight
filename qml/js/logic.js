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

// [hue, saturation, lightness] between 0 and 1 of a color. Qt 5.6 only gives its red, green, blue
function hsl(color) {
    var high = Math.max(color.r, color.g, color.b), low = Math.min(color.r, color.g, color.b)
    var lightness = (high + low) / 2, range = high - low
    if (range === 0)
        return [0, 0, lightness]
    var saturation = range / (1 - Math.abs(2 * lightness - 1))
    var hue
    if (high === color.r)
        hue = ((color.g - color.b) / range + 6) % 6
    else if (high === color.g)
        hue = (color.b - color.r) / range + 2
    else
        hue = (color.r - color.g) / range + 4
    return [hue / 6, Math.min(1, saturation), lightness]
}

// color of a sector, as [hue, saturation, lightness, opacity] between 0 and 1, from the
// [hue, saturation, lightness] of the highlight color of the theme.
// the sectors of a same first-ring item share a tint close to the highlight color, neighbours
// differ in hue and lightness, rings let more of the background through outward,
// and files are less saturated than folders
function sectorHsla(base, group, ring, folder) {
    var shifts = [0, 0.07, -0.07, 0.035, -0.035]
    var hue = (base[0] + shifts[group % shifts.length] + 1) % 1
    var saturation = Math.max(0.3, base[1]) * (folder ? 1 : 0.4)
    var lightness = Math.min(0.65, Math.max(0.45, base[2])) - (group % 2) * 0.13
    var opacity = Math.max(0.25, 0.9 * Math.pow(0.78, ring - 1))
    return [hue, saturation, lightness, opacity]
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

// address of a local file or folder, for the applications of the system
function fileUrl(path) {
    return "file://" + path.split("/").map(encodeURIComponent).join("/")
}

// what compressing adds to a name: folders go through tar, except for zip
function archiveExtension(format, folder) {
    if (format === "zip")
        return "zip"
    return (folder ? "tar." : "") + format
}

// part of a whole between 0 and 1, 0 when the whole is nothing
function fraction(part, whole) {
    return whole > 0 ? Math.min(1, Math.max(0, part / whole)) : 0
}

// shares of a storage, between 0 and 1 and summing to 1: [a folder in it, the rest of what is
// used, what is free]. all free when the storage is not known
function volumeShares(folder, used, total) {
    if (!(total > 0))
        return [0, 0, 1]
    var taken = fraction(used, total)
    var own = Math.min(taken, fraction(folder, total))
    return [own, taken - own, 1 - taken]
}

// how full a storage is: "3.2 GB of 16 GB"
function usageLabel(used, total) {
    return formatSize(used) + " of " + formatSize(total)
}

// "1 item", "12 items"
function itemCount(count) {
    return count === 1 ? "1 item" : count + " items"
}

// what a scan in progress shows: "1 234 items, 3.2 GB"
function scanLabel(items, bytes) {
    return itemCount(items) + ", " + formatSize(bytes)
}

// path of a folder as styled text for the cover, its last level in the `accent` color.
// the first `skipped` levels under the root are replaced by an ellipsis, for paths taking too much room
function pathMarkup(path, accent, skipped) {
    function escaped(text) {
        return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
    }
    var parts = path.split("/").filter(function(part) { return part.length > 0 })
    var last = parts.length > 0 ? parts.pop() : "/"
    var skip = Math.max(0, Math.min(skipped, parts.length))
    var before = (skip > 0 ? "\u2026/" : parts.length > 0 || last !== "/" ? "/" : "")
        + parts.slice(skip).map(function(part) { return part + "/" }).join("")
    return escaped(before) + '<font color="' + accent + '">' + escaped(last) + "</font>"
}
