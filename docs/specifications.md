# goal

make a local storage size vizualizer, like Filelight for Sailfish

- explore local file tree, starting from a current folder
- good ergonomy with chart browsing
- simple memory saving operations (deletion, compression)
- run either sandboxed or not

# non-goal

- file preview or manipulation other than saving memory
- impose a folder structure or produce a parallel index to the file systsem. only filesystem is the truth


# technical stack

- language: rust
- gui: QML with sailfish silica and qmetaobject-rs
- package: rpm, targetting chum repo

# layout

at the top is the current folder path
- levels items can be clicked to switch current folder to them
- a warning icon when the folder has changed since sizes where computed (compare folder/file modification dates against cache)

in the center is a recursive and interactive pie chart
- every element is shown propotionnaly to its byte size
- sub folders are recursively scanned
- sub folders are shown up to a level set (default 3)
- clicking a sector selects it and show details on it below
- clicking the center moves to parent folder

around chart (above and below in portrait and left and right in lanscape) are names of items in current folder with arrows pointing to them in the chart. names placement derived from the chart

below chart (or right in landscape mode) are the details of the selected sector:
- path
- set as current folder (if a folder)
- size, date, permission, etc
- option to delete (with standard sailfish confirmation)
- option to compress as zip, gz, 7z (tar first for folders if not zip)
  original is deleted on success

draging the screen down shows option to refresh and depth setting

# recursion

- the recursive size computation should be cached in RAM and app storage, always disposable and only for current folder. the refresh button drops and remakes it
- links and mount points are not followed to represent memory footprint of files once each
- unreachable folders are ignored an should be shown, but shown as a special zero-width sector (that renders with a thick line) with a warning color. when selected it lists the files/folders from which it couldn't recurse
- cache should contain even small sectors, but char only display those above 1%

# interaction

- when clicking the chart
  - selection is performed on release
  - sliding the click allows changing what is gonna be selected
  - it hovers the sectors that is gonna be selected
- scan shows a progress bar
- compression shows a progress bar
