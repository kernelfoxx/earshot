use std::io::Read;
use std::path::Path;

use super::ImportError;

/// A file to parse, possibly extracted from a zip archive.
pub struct InputFile {
    /// File name, or `archive.zip/inner/path` for archive entries.
    pub name: String,
    pub data: Vec<u8>,
}

/// Reads the selected paths. Zip archives (as delivered by Spotify and
/// Google) are expanded to the entries `wanted` accepts, so users can pick
/// the download as is.
pub fn read_inputs(
    paths: &[impl AsRef<Path>],
    wanted: impl Fn(&str) -> bool,
) -> Result<Vec<InputFile>, ImportError> {
    let mut out = Vec::new();
    for path in paths {
        let path = path.as_ref();
        let label = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let data = std::fs::read(path).map_err(|e| ImportError::Io(label.clone(), e))?;
        if data.starts_with(b"PK\x03\x04") {
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data))
                .map_err(|e| ImportError::Zip(label.clone(), e))?;
            for i in 0..zip.len() {
                let mut entry = zip
                    .by_index(i)
                    .map_err(|e| ImportError::Zip(label.clone(), e))?;
                let entry_name = entry
                    .name()
                    .map_err(|e| ImportError::Zip(label.clone(), e))?
                    .into_owned();
                if !entry.is_file() || !wanted(&entry_name) {
                    continue;
                }
                let name = format!("{label}/{entry_name}");
                let mut buf = Vec::with_capacity(entry.size() as usize);
                entry
                    .read_to_end(&mut buf)
                    .map_err(|e| ImportError::Io(name.clone(), e))?;
                out.push(InputFile { name, data: buf });
            }
        } else {
            out.push(InputFile { name: label, data });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn expands_zip_entries_and_reads_plain_files() {
        let dir = std::env::temp_dir().join(format!("tunelog-zip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("export.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file("data/Streaming_History_Audio_2024.json", opts)
                .unwrap();
            w.write_all(b"[]").unwrap();
            w.start_file("data/ReadMeFirst.pdf", opts).unwrap();
            w.write_all(b"%PDF").unwrap();
            w.finish().unwrap();
        }
        let plain = dir.join("plain.json");
        std::fs::write(&plain, b"[1]").unwrap();

        let files = read_inputs(&[&zip_path, &plain], |n| n.ends_with(".json")).unwrap();
        let names: Vec<_> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "export.zip/data/Streaming_History_Audio_2024.json",
                "plain.json"
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
