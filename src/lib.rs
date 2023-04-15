use std::collections::HashMap;
use std::io::{Seek, Read, SeekFrom};
use std::io::{Error, ErrorKind};

use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

const ADAT_MAGIC: [u8; 4] = [ 65, 68, 65, 84 ]; // ADAT
const ADAT_ENTRY_SIZE: u32 = 128 + 4 + 4 + 4 + 4; // raw sizeof PackageEntry

#[derive(Debug)]
pub struct Package<'b, T: Read + Seek> {
    cursor: &'b mut T,
    header: PackageHeader,
    entries: HashMap<String, PackageEntry>
}

#[derive(Debug)]
struct PackageHeader {
    magic: u32,
    toc_offset: u32,
    toc_length: u32,
    version: u32
}

#[derive(Debug)]
struct PackageEntry {
    name: [u8; 128], // file name
    offset: u32, // offset in DAT for the file
    length: usize, // decompressed length
    compressed_length: usize, // length in the DAT file
    u0: u32 // ??
}

// helper function for conversions
fn u32le_from_slice(acc: &[u8]) -> u32 {
    let mut bu4: [u8; 4] = [0; 4];
    bu4.copy_from_slice(acc);
    u32::from_le_bytes(bu4)
}

impl PackageEntry {
    pub fn get_name(&self) -> Result<&str, std::str::Utf8Error> { 
        core::str::from_utf8(&self.name).map(|s| {
            s.trim_end_matches(char::from(0))
        })
    }

    pub fn read_entry<T: Read + Seek>(&self, cursor: &mut T) -> std::io::Result<Vec<u8>> {
        let mut compressed_data: Vec<u8> = vec![0; self.compressed_length];

        cursor.seek(SeekFrom::Start(self.offset as u64))?;
        cursor.read_exact(&mut compressed_data)?;

        decompress_to_vec_zlib_with_limit(&compressed_data, self.length).map_err(|e| {
            Error::new(ErrorKind::Other, e.to_string())
        })
    }
}

impl PackageHeader {
    fn read_package_header<K: Read>(cursor: &mut K) -> std::io::Result<PackageHeader> {
        let mut result = PackageHeader {
            magic: 0,
            toc_offset: 0,
            toc_length: 0,
            version: 0
        };
        let mut buffer: [u8; 16] = [0; 16];
        cursor.read_exact(&mut buffer)?;

        // check magic
        if &buffer[0..4] != ADAT_MAGIC {
            return Err(Error::new(ErrorKind::Other,
                format!("ADAT magic mismatch, found: {:?}", &buffer[0..4])
            ));
        }

        result.magic = u32le_from_slice(&buffer[0..4]);
        result.toc_offset = u32le_from_slice(&buffer[4..8]);
        result.toc_length = u32le_from_slice(&buffer[8..12]);
        result.version = u32le_from_slice(&buffer[12..16]);

        if result.version != 9 {
            return Err(Error::new(ErrorKind::Other,
                format!("ADAT version mismatch, expected 9, found: {}", result.version)
            ));
        }

        Ok(result)
    }
}

impl PackageEntry {
    fn read_package_entry<'b, K: Read>(cursor: &'b mut K) -> std::io::Result<PackageEntry> {
        let mut entry: PackageEntry = PackageEntry {
            name: [0; 128],
            offset: 0,
            length: 0,
            compressed_length: 0,
            u0: 0
        };

        cursor.read_exact(&mut entry.name)?;

        let mut buffer: [u8; 16] = [0; 16];
        cursor.read_exact(&mut buffer)?;

        entry.offset = u32le_from_slice(&buffer[0..4]);
        entry.length = u32le_from_slice(&buffer[4..8]) as usize;
        entry.compressed_length = u32le_from_slice(&buffer[8..12]) as usize;
        entry.u0 = u32le_from_slice(&buffer[12..16]);

        Ok(entry)
    }

    fn read_package_entries<'b, K: Read>(cursor: &'b mut K, entry_count: u32) -> std::io::Result<Vec<PackageEntry>> {
        let mut entries: Vec<PackageEntry> = Vec::with_capacity(entry_count as usize);

        for _ in 0..entry_count {
            entries.push(PackageEntry::read_package_entry(cursor)?);
        }

        Ok(entries)
    }
}

impl<'b, T: Read + Seek>  Package<'b, T> {
    pub fn mount_from_cursor(cursor: &'b mut T) -> std::io::Result<Self> {
        cursor.seek(SeekFrom::Start(0))?;

        let header: PackageHeader = PackageHeader::read_package_header(cursor)?;
        let entry_count = header.toc_length / ADAT_ENTRY_SIZE;

        if entry_count == 0 {
            return Err(Error::new(ErrorKind::Other, "empty toc"));
        }

        cursor.seek(SeekFrom::Start(header.toc_offset as u64))?;
        let entries = PackageEntry::read_package_entries(cursor, entry_count)?;

        let mut entrymap: HashMap<String, PackageEntry> = HashMap::with_capacity(entries.len());
        for entry in entries {
            let path = entry.get_name().map_err(|e| {
                Error::new(ErrorKind::Other, e)
            })?;
            entrymap.insert(path.to_string(), entry);
        }

        let result = Package {
            cursor: cursor,
            header: header,
            entries: entrymap
        };

        Ok(result)
    }

    pub fn list_entries(&self) -> Vec<String> {
        self.entries.keys().map(|k| k.to_string()).collect()
    }

    pub fn read_entry(&mut self, entry_path: &str) -> std::io::Result<Vec<u8>> {
        self.entries.get(entry_path).ok_or(Error::new(
            ErrorKind::Other, "entry not found"
        )).and_then(|pe| {
            pe.read_entry(self.cursor)
        })
    }

    pub fn read_text_entry(&mut self, entry_path: &str) -> std::io::Result<String> {
        self.read_entry(entry_path).and_then(|v| {
            String::from_utf8(v).map_err(|e| {
                Error::new(ErrorKind::Other, e)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn it_works() {
        let mut file = File::open("TEST.dat").unwrap();
        let mut result = Package::mount_from_cursor(&mut file).unwrap();

        assert_eq!(result.header.magic, 0x54414441); // ADAT
        assert_eq!(result.header.version, 9); // expected

        let names = result.list_entries();
        assert_eq!(names.len(), 1);

        assert_eq!(names[0], "some/path/foo.txt");

        let mut expected_content = "".to_string();
        for idx in 1..6 {
            expected_content = format!("{}\n{} {}", expected_content, idx, "hello world from a test file!");
        }
        expected_content = expected_content + "\n";
        assert_eq!(result.read_text_entry("some/path/foo.txt").unwrap(), expected_content);

        drop(file);
    }
}
