//! compression as zip, gz or 7z (tar first for folders if not zip), with progress.
//! the original is deleted on success
//!
//! the archive is written next to the original under a temporary name, read back entirely
//! to check it, renamed, and only then the original is deleted. a failure or a cancellation
//! at any point before that leaves the original untouched and no archive.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, Metadata};
use std::io::{self, BufWriter, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::time::Instant;

use super::delete;
use crate::worker::Cancel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Zip,
    Gz,
    SevenZ,
}

impl Format {
    /// from the name used by the interface
    pub fn parse(name: &str) -> Option<Format> {
        match name {
            "zip" => Some(Format::Zip),
            "gz" => Some(Format::Gz),
            "7z" => Some(Format::SevenZ),
            _ => None,
        }
    }

    /// what is added to the name of the original. folders go through tar, except for zip
    pub fn extension(self, folder: bool) -> &'static str {
        match (self, folder) {
            (Format::Zip, _) => "zip",
            (Format::Gz, false) => "gz",
            (Format::Gz, true) => "tar.gz",
            (Format::SevenZ, false) => "7z",
            (Format::SevenZ, true) => "tar.7z",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// stopped on request. nothing changed
    Cancelled,
    /// with what to tell the user. nothing changed, unless the message says otherwise
    Failed(String),
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Failure::Failed(error.to_string())
    }
}

fn failed<T>(message: impl Into<String>) -> Result<T, Failure> {
    Err(Failure::Failed(message.into()))
}

/// 7z sizes are counted in the machine's word by the library: on 32 bits, less than 4 GB
const SEVENZ_LIMIT_32: u64 = 3_500_000_000;

/// time between two progress reports, milliseconds
const REPORT_EVERY: u64 = 100;

/// counts the bytes going through, reports the progress, and stops on cancellation.
/// reading the original and reading the archive back each make half of the progress
struct Meter<'a> {
    done: AtomicU64,
    /// bytes of the original, counted twice
    total: u64,
    progress: &'a (dyn Fn(f64) + Sync),
    cancel: &'a Cancel,
    started: Instant,
    reported: AtomicU64,
}

impl<'a> Meter<'a> {
    fn new(total: u64, progress: &'a (dyn Fn(f64) + Sync), cancel: &'a Cancel) -> Self {
        Meter {
            done: AtomicU64::new(0),
            total: total.saturating_mul(2).max(1),
            progress,
            cancel,
            started: Instant::now(),
            reported: AtomicU64::new(0),
        }
    }

    fn check(&self) -> io::Result<()> {
        if self.cancel.is_cancelled() {
            Err(io::Error::new(io::ErrorKind::Other, "cancelled"))
        } else {
            Ok(())
        }
    }

    fn add(&self, bytes: u64) -> io::Result<()> {
        self.check()?;
        let done = self.done.fetch_add(bytes, Ordering::Relaxed) + bytes;
        let now = self.started.elapsed().as_millis() as u64;
        let last = self.reported.load(Ordering::Relaxed);
        if now >= last + REPORT_EVERY
            && self.reported.compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed).is_ok()
        {
            (self.progress)((done as f64 / self.total as f64).min(1.0));
        }
        Ok(())
    }
}

/// a reader counted by a meter
struct Metered<'a, R> {
    inner: R,
    meter: &'a Meter<'a>,
    read: u64,
}

impl<'a, R> Metered<'a, R> {
    fn new(inner: R, meter: &'a Meter<'a>) -> Self {
        Metered { inner, meter, read: 0 }
    }
}

impl<R: Read> Read for Metered<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.read += count as u64;
        self.meter.add(count as u64)?;
        Ok(count)
    }
}

/// checksum and length of what goes through a reader or a writer
struct Summed<T> {
    inner: T,
    sum: crc32fast::Hasher,
    length: u64,
}

impl<T> Summed<T> {
    fn new(inner: T) -> Self {
        Summed { inner, sum: crc32fast::Hasher::new(), length: 0 }
    }

    fn summary(&self) -> (u32, u64) {
        (self.sum.clone().finalize(), self.length)
    }
}

impl<T: Read> Read for Summed<T> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.sum.update(&buffer[..count]);
        self.length += count as u64;
        Ok(count)
    }
}

impl<T: Write> Write for Summed<T> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let count = self.inner.write(buffer)?;
        self.sum.update(&buffer[..count]);
        self.length += count as u64;
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// checksum and length of everything a reader gives
fn summarize(reader: impl Read) -> io::Result<(u32, u64)> {
    let mut summed = Summed::new(reader);
    io::copy(&mut summed, &mut io::sink())?;
    Ok(summed.summary())
}

/// bytes of the files in `path`, and whether all names are valid text
fn measure(path: &Path, cancel: &Cancel) -> Result<(u64, bool), Failure> {
    let mut total = 0u64;
    let mut text = path.file_name().map_or(false, |name| name.to_str().is_some());
    let mut pending = vec![path.to_path_buf()];
    while let Some(current) = pending.pop() {
        if cancel.is_cancelled() {
            return Err(Failure::Cancelled);
        }
        let metadata = fs::symlink_metadata(&current)?;
        if metadata.is_dir() {
            for entry in fs::read_dir(&current)? {
                let entry = entry?;
                text &= entry.file_name().to_str().is_some();
                pending.push(entry.path());
            }
        } else if metadata.is_file() {
            total += metadata.len();
        }
    }
    Ok((total, text))
}

/// a name not taken in `folder`: `name`, or `stem (2).extension`, `stem (3).extension`...
fn free_name(folder: &Path, stem: &OsStr, extension: &str) -> PathBuf {
    let mut number = 1u32;
    loop {
        let mut name = stem.to_os_string();
        if number > 1 {
            name.push(format!(" ({number})"));
        }
        name.push(".");
        name.push(extension);
        let path = folder.join(name);
        if fs::symlink_metadata(&path).is_err() {
            return path;
        }
        number += 1;
    }
}

/// compress the file or folder at `path` into an archive next to it, then delete it.
///
/// `progress` receives the share of the work done, from 0 to 1, from any thread.
/// gives the path of the archive
pub fn compress(
    path: &Path,
    format: Format,
    progress: &(dyn Fn(f64) + Sync),
    cancel: &Cancel,
) -> Result<PathBuf, Failure> {
    let metadata = fs::symlink_metadata(path)?;
    let folder = metadata.is_dir();
    if !folder && !metadata.is_file() {
        return failed("Only files and folders can be compressed");
    }
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return failed("This cannot be compressed");
    };
    let (total, text) = measure(path, cancel)?;
    match format {
        Format::Zip if !text => {
            return failed("A name is not valid text, which zip cannot keep. Use another format");
        }
        Format::SevenZ if name.to_str().is_none() => {
            return failed("The name is not valid text, which 7z cannot keep. Use another format");
        }
        Format::SevenZ if cfg!(target_pointer_width = "32") && total > SEVENZ_LIMIT_32 => {
            return failed("Too big for 7z on this device. Use another format");
        }
        _ => {}
    }

    let archive = free_name(parent, name, format.extension(folder));
    let mut part_name = OsString::from(".");
    part_name.push(archive.file_name().unwrap_or(name));
    part_name.push(".part");
    let part = parent.join(part_name);
    // only this function makes files of that name: one left by a killed run
    let _ = fs::remove_file(&part);

    let meter = Meter::new(total, progress, cancel);
    let written = write_archive(path, name, &metadata, format, &part, &meter)
        .and_then(|written| {
            if verify(&part, format, &meter)? == written {
                Ok(())
            } else {
                Err(io::Error::new(io::ErrorKind::Other, "the archive read back differs from what was written"))
            }
        })
        .and_then(|()| meter.check())
        .and_then(|()| fs::rename(&part, &archive));
    if let Err(error) = written {
        let _ = fs::remove_file(&part);
        return Err(if cancel.is_cancelled() {
            Failure::Cancelled
        } else {
            Failure::Failed(error.to_string())
        });
    }
    progress(1.0);

    if let Err(error) = delete::remove(path) {
        return failed(format!(
            "The archive was made, but the original could not be removed: {error}"
        ));
    }
    Ok(archive)
}

/// what was put in an archive, to compare with what is read back
#[derive(Debug, PartialEq)]
enum Written {
    /// checksum and length of the single stream of a gz or 7z archive
    Stream(u32, u64),
    /// number of entries and bytes of files of a zip archive
    Entries(usize, u64),
}

fn write_archive(
    path: &Path,
    name: &OsStr,
    metadata: &Metadata,
    format: Format,
    part: &Path,
    meter: &Meter,
) -> io::Result<Written> {
    let folder = metadata.is_dir();
    // private until it is complete
    let file = fs::OpenOptions::new().write(true).create_new(true).open(part)?;
    match format {
        Format::Zip => write_zip(path, name, file, meter),
        Format::Gz => {
            let encoder = if folder {
                flate2::write::GzEncoder::new(file, flate2::Compression::default())
            } else {
                flate2::GzBuilder::new()
                    .filename(name.as_bytes())
                    .mtime(metadata.mtime().clamp(0, u32::MAX as i64) as u32)
                    .write(file, flate2::Compression::default())
            };
            let mut summed = Summed::new(BufWriter::new(encoder));
            if folder {
                summed = write_tar(path, name, summed, meter)?;
            } else {
                copy_file(path, metadata, &mut summed, meter)?;
            }
            let summary = summed.summary();
            let encoder = summed.inner.into_inner().map_err(|error| error.into_error())?;
            encoder.finish()?.sync_all()?;
            Ok(Written::Stream(summary.0, summary.1))
        }
        Format::SevenZ => {
            let mut writer = sevenz_rust::SevenZWriter::new(file).map_err(sevenz_error)?;
            writer.set_encrypt_header(false);
            let lossy = name.to_string_lossy().into_owned();
            let summary = if folder {
                let mut entry = sevenz_rust::SevenZArchiveEntry::new();
                entry.name = format!("{lossy}.tar");
                entry.has_stream = true;
                // the tar is made by another thread and read here as the content of the entry
                let (sender, receiver) = sync_channel::<Vec<u8>>(8);
                std::thread::scope(|scope| {
                    let maker = scope.spawn(move || -> io::Result<()> {
                        let pipe = BufWriter::with_capacity(PIPE_PIECE, PipeWriter(sender));
                        let pipe = write_tar(path, name, pipe, meter)?;
                        pipe.into_inner().map_err(|error| error.into_error())?;
                        Ok(())
                    });
                    let mut summed = Summed::new(PipeReader::new(receiver));
                    let pushed = writer.push_archive_entry(entry, Some(&mut summed)).map(|_| ());
                    // unblocks the maker if the entry was not read to its end
                    drop(summed.inner.receiver.take());
                    let made = maker
                        .join()
                        .unwrap_or_else(|_| Err(io::Error::new(io::ErrorKind::Other, "tar failed")));
                    // the maker's error is the cause when both failed
                    made?;
                    pushed.map_err(sevenz_error)?;
                    Ok::<_, io::Error>(summed.summary())
                })?
            } else {
                let entry = sevenz_rust::SevenZArchiveEntry::from_path(path, lossy);
                let source = Metered::new(File::open(path)?, meter);
                let mut summed = Summed::new(source.take(metadata.len()));
                writer.push_archive_entry(entry, Some(&mut summed)).map_err(sevenz_error)?;
                if summed.length != metadata.len() {
                    return Err(changed(path));
                }
                summed.summary()
            };
            writer.finish()?.sync_all()?;
            Ok(Written::Stream(summary.0, summary.1))
        }
    }
}

fn sevenz_error(error: sevenz_rust::Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, error.to_string())
}

fn changed(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::Other,
        format!("{} changed while it was read", path.display()),
    )
}

/// copy exactly the content a file had when it was examined
fn copy_file(path: &Path, metadata: &Metadata, out: &mut impl Write, meter: &Meter) -> io::Result<()> {
    let mut source = Metered::new(File::open(path)?, meter).take(metadata.len());
    if io::copy(&mut source, out)? != metadata.len() {
        return Err(changed(path));
    }
    Ok(())
}

/// what is in a folder, in the order of the names
fn sorted_entries(folder: &Path) -> io::Result<Vec<OsString>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(folder)? {
        names.push(entry?.file_name());
    }
    names.sort();
    Ok(names)
}

/// write the folder `path` as a tar named `name` inside
fn write_tar<W: Write>(path: &Path, name: &OsStr, out: W, meter: &Meter) -> io::Result<W> {
    let mut builder = tar::Builder::new(out);
    builder.follow_symlinks(false);
    builder.mode(tar::HeaderMode::Complete);
    // (on the filesystem, in the archive), last one first
    let mut pending = vec![(path.to_path_buf(), PathBuf::from(name))];
    while let Some((source, inside)) = pending.pop() {
        meter.check()?;
        let metadata = fs::symlink_metadata(&source)?;
        if metadata.is_dir() {
            builder.append_dir(&inside, &source)?;
            for name in sorted_entries(&source)?.into_iter().rev() {
                pending.push((source.join(&name), inside.join(&name)));
            }
        } else if metadata.is_file() {
            let mut header = tar::Header::new_gnu();
            header.set_metadata(&metadata);
            let mut content = Metered::new(File::open(&source)?, meter).take(metadata.len());
            builder.append_data(&mut header, &inside, &mut content)?;
            if content.get_ref().read != metadata.len() {
                return Err(changed(&source));
            }
        } else {
            // links are kept as links
            builder.append_path_with_name(&source, &inside)?;
        }
    }
    builder.into_inner()
}

fn zip_error(error: zip::result::ZipError) -> io::Error {
    match error {
        zip::result::ZipError::Io(error) => error,
        other => io::Error::new(io::ErrorKind::Other, other.to_string()),
    }
}

fn text(name: &OsStr) -> io::Result<&str> {
    name.to_str().ok_or_else(|| io::Error::new(io::ErrorKind::Other, "a name is not valid text"))
}

/// zip stores local dates, between 1980 and 2107
fn zip_date(seconds: i64) -> Option<zip::DateTime> {
    let seconds: libc::time_t = seconds.try_into().ok()?;
    let mut local: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&seconds, &mut local) }.is_null() {
        return None;
    }
    zip::DateTime::from_date_and_time(
        (local.tm_year + 1900).try_into().ok()?,
        (local.tm_mon + 1).try_into().ok()?,
        local.tm_mday.try_into().ok()?,
        local.tm_hour.try_into().ok()?,
        local.tm_min.try_into().ok()?,
        local.tm_sec.min(59).try_into().ok()?,
    )
    .ok()
}

fn zip_options(metadata: &Metadata) -> zip::write::FileOptions {
    let options = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(metadata.mode() & 0o7777)
        .large_file(metadata.len() >= u32::MAX as u64);
    match zip_date(metadata.mtime()) {
        Some(date) => options.last_modified_time(date),
        None => options,
    }
}

fn write_zip(path: &Path, name: &OsStr, file: File, meter: &Meter) -> io::Result<Written> {
    let mut writer = zip::ZipWriter::new(BufWriter::new(file));
    let mut entries = 0usize;
    let mut bytes = 0u64;
    // (on the filesystem, in the archive), last one first
    let mut pending = vec![(path.to_path_buf(), text(name)?.to_owned())];
    while let Some((source, inside)) = pending.pop() {
        meter.check()?;
        let metadata = fs::symlink_metadata(&source)?;
        let options = zip_options(&metadata);
        if metadata.is_dir() {
            writer.add_directory(inside.as_str(), options).map_err(zip_error)?;
            for name in sorted_entries(&source)?.into_iter().rev() {
                let child = format!("{inside}/{}", text(&name)?);
                pending.push((source.join(&name), child));
            }
        } else if metadata.is_file() {
            writer.start_file(inside.as_str(), options).map_err(zip_error)?;
            copy_file(&source, &metadata, &mut writer, meter)?;
            bytes += metadata.len();
        } else if metadata.file_type().is_symlink() {
            let target = fs::read_link(&source)?;
            let target = text(target.as_os_str())?.to_owned();
            bytes += target.len() as u64;
            writer.add_symlink(inside.as_str(), target, options).map_err(zip_error)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("{} is not a file, zip cannot keep it", source.display()),
            ));
        }
        entries += 1;
    }
    let file = writer.finish().map_err(zip_error)?;
    file.into_inner().map_err(|error| error.into_error())?.sync_all()?;
    Ok(Written::Entries(entries, bytes))
}

/// read an archive back entirely
fn verify(part: &Path, format: Format, meter: &Meter) -> io::Result<Written> {
    match format {
        Format::Zip => {
            let mut archive = zip::ZipArchive::new(io::BufReader::new(File::open(part)?))
                .map_err(zip_error)?;
            let mut bytes = 0u64;
            for index in 0..archive.len() {
                let entry = archive.by_index(index).map_err(zip_error)?;
                // reading to the end compares the checksum kept in the archive
                bytes += io::copy(&mut Metered::new(entry, meter), &mut io::sink())?;
            }
            Ok(Written::Entries(archive.len(), bytes))
        }
        Format::Gz => {
            let decoder = flate2::read::GzDecoder::new(io::BufReader::new(File::open(part)?));
            let (sum, length) = summarize(Metered::new(decoder, meter))?;
            Ok(Written::Stream(sum, length))
        }
        Format::SevenZ => {
            let mut reader =
                sevenz_rust::SevenZReader::open(part, sevenz_rust::Password::empty())
                    .map_err(sevenz_error)?;
            let mut streams = Vec::new();
            reader
                .for_each_entries(|_, content| {
                    streams.push(summarize(Metered::new(content, meter))?);
                    Ok(true)
                })
                .map_err(sevenz_error)?;
            match streams.as_slice() {
                [(sum, length)] => Ok(Written::Stream(*sum, *length)),
                _ => Err(io::Error::new(io::ErrorKind::Other, "unexpected content in the archive")),
            }
        }
    }
}

/// size of the pieces going through the pipe
const PIPE_PIECE: usize = 256 * 1024;

/// writing end of a pipe between two threads
struct PipeWriter(SyncSender<Vec<u8>>);

impl Write for PipeWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0
            .send(buffer.to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "the reader is gone"))?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// reading end: gives the pieces in order, and ends when the writer is dropped
struct PipeReader {
    receiver: Option<Receiver<Vec<u8>>>,
    piece: Vec<u8>,
    position: usize,
}

impl PipeReader {
    fn new(receiver: Receiver<Vec<u8>>) -> Self {
        PipeReader { receiver: Some(receiver), piece: Vec::new(), position: 0 }
    }
}

impl Read for PipeReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        while self.position == self.piece.len() {
            match self.receiver.as_ref().and_then(|receiver| receiver.recv().ok()) {
                Some(piece) => {
                    self.piece = piece;
                    self.position = 0;
                }
                None => return Ok(0),
            }
        }
        let count = buffer.len().min(self.piece.len() - self.position);
        buffer[..count].copy_from_slice(&self.piece[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::collections::BTreeMap;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::Mutex;

    const FORMATS: [Format; 3] = [Format::Zip, Format::Gz, Format::SevenZ];

    /// content that compresses, but not to nothing
    fn content(size: usize, seed: u8) -> Vec<u8> {
        (0..size).map(|index| (index as u32).wrapping_mul(31).wrapping_add(seed as u32) as u8 / 7).collect()
    }

    fn quiet(path: &Path, format: Format) -> Result<PathBuf, Failure> {
        compress(path, format, &|_| {}, &Cancel::default())
    }

    /// a folder with files, a sub folder, an empty folder, an empty file and a link
    fn make_folder(root: &Path) -> BTreeMap<String, Vec<u8>> {
        fs::create_dir_all(root.join("album/sub/deeper")).unwrap();
        fs::create_dir_all(root.join("album/empty")).unwrap();
        let mut files = BTreeMap::new();
        for (name, size, seed) in [
            ("album/one.txt", 70_000, 1),
            ("album/sub/two.bin", 200_000, 2),
            ("album/sub/deeper/three", 5, 3),
            ("album/sub/nothing", 0, 4),
            ("album/with space é.txt", 1_000, 5),
        ] {
            let data = content(size, seed);
            fs::write(root.join(name), &data).unwrap();
            files.insert(name.to_owned(), data);
        }
        symlink("one.txt", root.join("album/link")).unwrap();
        fs::set_permissions(root.join("album/one.txt"), fs::Permissions::from_mode(0o640)).unwrap();
        files
    }

    /// files of a tar: path to content, links as `-> target`, folders as `/`
    fn read_tar(data: &[u8]) -> BTreeMap<String, Vec<u8>> {
        let mut found = BTreeMap::new();
        let mut archive = tar::Archive::new(data);
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            let kind = entry.header().entry_type();
            if kind.is_dir() {
                found.insert(path.trim_end_matches('/').to_owned(), b"/".to_vec());
            } else if kind.is_symlink() {
                let target = entry.link_name().unwrap().unwrap().to_string_lossy().into_owned();
                found.insert(path, format!("-> {target}").into_bytes());
            } else {
                let mut data = Vec::new();
                entry.read_to_end(&mut data).unwrap();
                found.insert(path, data);
            }
        }
        found
    }

    fn read_zip(archive: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut found = BTreeMap::new();
        let mut zip = zip::ZipArchive::new(File::open(archive).unwrap()).unwrap();
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).unwrap();
            let name = entry.name().trim_end_matches('/').to_owned();
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            let link = entry.unix_mode().map_or(false, |mode| mode & 0o170000 == 0o120000);
            if entry.is_dir() {
                found.insert(name, b"/".to_vec());
            } else if link {
                found.insert(name, format!("-> {}", String::from_utf8(data).unwrap()).into_bytes());
            } else {
                found.insert(name, data);
            }
        }
        found
    }

    fn read_gz(archive: &Path) -> Vec<u8> {
        let mut data = Vec::new();
        flate2::read::GzDecoder::new(File::open(archive).unwrap()).read_to_end(&mut data).unwrap();
        data
    }

    /// entries of a 7z: name to content
    fn read_7z(archive: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut found = BTreeMap::new();
        let mut reader =
            sevenz_rust::SevenZReader::open(archive, sevenz_rust::Password::empty()).unwrap();
        reader
            .for_each_entries(|entry, content| {
                let mut data = Vec::new();
                content.read_to_end(&mut data)?;
                found.insert(entry.name().to_owned(), data);
                Ok(true)
            })
            .unwrap();
        found
    }

    /// what the folder made by `make_folder` must give in an archive
    fn expected_folder(files: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
        let mut expected = files.clone();
        for folder in ["album", "album/sub", "album/sub/deeper", "album/empty"] {
            expected.insert(folder.to_owned(), b"/".to_vec());
        }
        expected.insert("album/link".to_owned(), b"-> one.txt".to_vec());
        expected
    }

    fn names_in(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn names_of_the_archives() {
        assert_eq!(Format::parse("zip"), Some(Format::Zip));
        assert_eq!(Format::parse("gz"), Some(Format::Gz));
        assert_eq!(Format::parse("7z"), Some(Format::SevenZ));
        assert_eq!(Format::parse("rar"), None);
        assert_eq!(Format::Zip.extension(true), "zip");
        assert_eq!(Format::Zip.extension(false), "zip");
        assert_eq!(Format::Gz.extension(true), "tar.gz");
        assert_eq!(Format::Gz.extension(false), "gz");
        assert_eq!(Format::SevenZ.extension(true), "tar.7z");
        assert_eq!(Format::SevenZ.extension(false), "7z");
    }

    #[test]
    fn a_file_is_replaced_by_its_archive() {
        for format in FORMATS {
            let temp = TempDir::new();
            let data = content(300_000, 9);
            let original = temp.path().join("report.txt");
            fs::write(&original, &data).unwrap();

            let archive = quiet(&original, format).unwrap();
            assert_eq!(archive, temp.path().join(format!("report.txt.{}", format.extension(false))));
            assert_eq!(names_in(temp.path()), [archive.file_name().unwrap().to_str().unwrap()]);
            assert!(fs::metadata(&archive).unwrap().len() < data.len() as u64);
            let back = match format {
                Format::Zip => read_zip(&archive).remove("report.txt").unwrap(),
                Format::Gz => read_gz(&archive),
                Format::SevenZ => read_7z(&archive).remove("report.txt").unwrap(),
            };
            assert!(back == data, "{format:?}: content differs");
        }
    }

    #[test]
    fn a_folder_is_replaced_by_its_archive() {
        for format in FORMATS {
            let temp = TempDir::new();
            let files = make_folder(temp.path());
            let archive = quiet(&temp.path().join("album"), format).unwrap();
            assert_eq!(archive, temp.path().join(format!("album.{}", format.extension(true))));
            assert_eq!(names_in(temp.path()), [archive.file_name().unwrap().to_str().unwrap()]);
            let back = match format {
                Format::Zip => read_zip(&archive),
                Format::Gz => read_tar(&read_gz(&archive)),
                Format::SevenZ => {
                    let mut entries = read_7z(&archive);
                    assert_eq!(entries.len(), 1);
                    read_tar(&entries.remove("album.tar").unwrap())
                }
            };
            let expected = expected_folder(&files);
            assert_eq!(
                back.keys().collect::<Vec<_>>(),
                expected.keys().collect::<Vec<_>>(),
                "{format:?}"
            );
            for (name, data) in &expected {
                assert!(&back[name] == data, "{format:?}: {name} differs");
            }
        }
    }

    #[test]
    fn permissions_and_dates_are_kept() {
        let temp = TempDir::new();
        make_folder(temp.path());
        let date = fs::metadata(temp.path().join("album/one.txt")).unwrap().mtime();
        let archive = quiet(&temp.path().join("album"), Format::Gz).unwrap();
        let data = read_gz(&archive);
        let mut tar = tar::Archive::new(data.as_slice());
        let entry = tar
            .entries()
            .unwrap()
            .map(|entry| entry.unwrap())
            .find(|entry| entry.path().unwrap().ends_with("one.txt"))
            .unwrap();
        assert_eq!(entry.header().mode().unwrap() & 0o777, 0o640);
        assert_eq!(entry.header().mtime().unwrap() as i64, date);

        let temp = TempDir::new();
        make_folder(temp.path());
        let archive = quiet(&temp.path().join("album"), Format::Zip).unwrap();
        let mut zip = zip::ZipArchive::new(File::open(archive).unwrap()).unwrap();
        let entry = zip.by_name("album/one.txt").unwrap();
        assert_eq!(entry.unix_mode().unwrap() & 0o777, 0o640);
        assert!(entry.last_modified().year() >= 2026);
    }

    #[test]
    fn empty_things_can_be_compressed() {
        for format in FORMATS {
            let temp = TempDir::new();
            fs::write(temp.path().join("empty"), b"").unwrap();
            fs::create_dir(temp.path().join("hollow")).unwrap();
            let file = quiet(&temp.path().join("empty"), format).unwrap();
            let folder = quiet(&temp.path().join("hollow"), format).unwrap();
            assert!(file.exists() && folder.exists(), "{format:?}");
            assert_eq!(names_in(temp.path()).len(), 2, "{format:?}");
        }
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let temp = TempDir::new();
        let original = temp.path().join("notes");
        fs::write(temp.path().join("notes.gz"), b"already there").unwrap();
        fs::write(temp.path().join("notes (2).gz"), b"this one too").unwrap();
        fs::write(&original, content(10_000, 1)).unwrap();
        let archive = quiet(&original, Format::Gz).unwrap();
        assert_eq!(archive, temp.path().join("notes (3).gz"));
        assert_eq!(fs::read(temp.path().join("notes.gz")).unwrap(), b"already there");
        assert_eq!(fs::read(temp.path().join("notes (2).gz")).unwrap(), b"this one too");
        assert_eq!(read_gz(&archive), content(10_000, 1));
    }

    #[test]
    fn progress_goes_up_to_the_end() {
        for format in FORMATS {
            let temp = TempDir::new();
            make_folder(temp.path());
            let reports = Mutex::new(Vec::new());
            compress(
                &temp.path().join("album"),
                format,
                &|share| reports.lock().unwrap().push(share),
                &Cancel::default(),
            )
            .unwrap();
            let reports = reports.into_inner().unwrap();
            assert_eq!(reports.last(), Some(&1.0), "{format:?}");
            assert!(reports.iter().all(|&share| (0.0..=1.0).contains(&share)));
            assert!(reports.windows(2).all(|pair| pair[0] <= pair[1]), "{format:?}: {reports:?}");
        }
    }

    #[test]
    fn cancellation_changes_nothing() {
        for format in FORMATS {
            let temp = TempDir::new();
            let files = make_folder(temp.path());
            let cancel = Cancel::default();
            cancel.cancel();
            let result = compress(&temp.path().join("album"), format, &|_| {}, &cancel);
            assert_eq!(result, Err(Failure::Cancelled), "{format:?}");
            assert_eq!(names_in(temp.path()), ["album"], "{format:?}");
            for (name, data) in &files {
                assert!(&fs::read(temp.path().join(name)).unwrap() == data);
            }
        }
    }

    #[test]
    fn cancellation_in_the_middle_leaves_no_archive() {
        for format in FORMATS {
            let temp = TempDir::new();
            let original = temp.path().join("big");
            let data = content(3_000_000, 3);
            fs::write(&original, &data).unwrap();
            let cancel = Cancel::default();
            let seen = AtomicU64::new(0);
            // stop as soon as some of it was read
            let result = compress(
                &original,
                format,
                &|share| {
                    if share > 0.0 {
                        seen.fetch_add(1, Ordering::Relaxed);
                        cancel.cancel();
                    }
                },
                &cancel,
            );
            if seen.load(Ordering::Relaxed) == 0 {
                // too fast for a report before the end: nothing to check
                assert!(result.is_ok());
                continue;
            }
            assert_eq!(result, Err(Failure::Cancelled), "{format:?}");
            assert_eq!(names_in(temp.path()), ["big"], "{format:?}");
            assert!(fs::read(&original).unwrap() == data);
        }
    }

    #[test]
    fn a_failure_keeps_the_original() {
        // root writes anywhere
        if fs::metadata("/proc/self").map(|metadata| metadata.uid() == 0).unwrap_or(false) {
            return;
        }
        for format in FORMATS {
            let temp = TempDir::new();
            let folder = temp.path().join("locked");
            fs::create_dir(&folder).unwrap();
            let original = folder.join("file");
            fs::write(&original, content(10_000, 1)).unwrap();
            fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
            let result = quiet(&original, format);
            fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
            assert!(matches!(result, Err(Failure::Failed(_))), "{format:?}: {result:?}");
            assert_eq!(names_in(&folder), ["file"], "{format:?}");
            assert_eq!(fs::read(&original).unwrap(), content(10_000, 1));
        }
    }

    #[test]
    fn an_unreadable_file_inside_keeps_everything() {
        if fs::metadata("/proc/self").map(|metadata| metadata.uid() == 0).unwrap_or(false) {
            return;
        }
        for format in FORMATS {
            let temp = TempDir::new();
            make_folder(temp.path());
            let secret = temp.path().join("album/sub/two.bin");
            fs::set_permissions(&secret, fs::Permissions::from_mode(0o000)).unwrap();
            let result = quiet(&temp.path().join("album"), format);
            fs::set_permissions(&secret, fs::Permissions::from_mode(0o644)).unwrap();
            assert!(matches!(result, Err(Failure::Failed(_))), "{format:?}: {result:?}");
            assert_eq!(names_in(temp.path()), ["album"], "{format:?}");
            assert!(temp.path().join("album/one.txt").exists());
            assert!(secret.exists());
        }
    }

    #[test]
    fn links_and_missing_things_are_refused() {
        let temp = TempDir::new();
        fs::write(temp.path().join("target"), b"content").unwrap();
        symlink("target", temp.path().join("link")).unwrap();
        for format in FORMATS {
            assert!(matches!(quiet(&temp.path().join("link"), format), Err(Failure::Failed(_))));
            assert!(matches!(quiet(&temp.path().join("missing"), format), Err(Failure::Failed(_))));
        }
        assert_eq!(names_in(temp.path()), ["link", "target"]);
    }

    #[test]
    fn names_that_are_not_text() {
        let temp = TempDir::new();
        let odd = OsString::from_vec(b"odd\xff\xfename".to_vec());
        fs::create_dir(temp.path().join("folder")).unwrap();
        fs::write(temp.path().join("folder").join(&odd), content(1000, 1)).unwrap();
        // zip cannot keep the name: refused, nothing changed
        assert!(matches!(quiet(&temp.path().join("folder"), Format::Zip), Err(Failure::Failed(_))));
        assert_eq!(names_in(temp.path()), ["folder"]);
        // tar keeps it as it is
        let archive = quiet(&temp.path().join("folder"), Format::Gz).unwrap();
        let data = read_gz(&archive);
        let mut tar = tar::Archive::new(data.as_slice());
        let found = tar.entries().unwrap().any(|entry| {
            entry.unwrap().path_bytes().ends_with(b"odd\xff\xfename")
        });
        assert!(found);

        // a file whose own name is not text cannot be a 7z entry
        let file = temp.path().join(&odd);
        fs::write(&file, content(1000, 1)).unwrap();
        assert!(matches!(quiet(&file, Format::SevenZ), Err(Failure::Failed(_))));
        assert!(file.exists());
    }

    #[test]
    fn a_leftover_of_a_killed_run_is_replaced() {
        let temp = TempDir::new();
        let original = temp.path().join("notes");
        fs::write(&original, content(10_000, 1)).unwrap();
        fs::write(temp.path().join(".notes.gz.part"), b"half an archive").unwrap();
        let archive = quiet(&original, Format::Gz).unwrap();
        assert_eq!(names_in(temp.path()), ["notes.gz"]);
        assert_eq!(read_gz(&archive), content(10_000, 1));
    }

    #[test]
    fn the_pipe_gives_what_was_written_then_ends() {
        let (sender, receiver) = sync_channel::<Vec<u8>>(2);
        let writer = std::thread::spawn(move || {
            let mut pipe = PipeWriter(sender);
            for piece in 0..50u8 {
                pipe.write_all(&vec![piece; 1000]).unwrap();
            }
        });
        let mut data = Vec::new();
        PipeReader::new(receiver).read_to_end(&mut data).unwrap();
        writer.join().unwrap();
        assert_eq!(data.len(), 50_000);
        assert!(data.chunks(1000).enumerate().all(|(index, chunk)| chunk.iter().all(|&byte| byte == index as u8)));

        // a writer whose reader is gone fails instead of waiting
        let (sender, receiver) = sync_channel::<Vec<u8>>(1);
        drop(receiver);
        assert!(PipeWriter(sender).write_all(b"data").is_err());
    }
}
