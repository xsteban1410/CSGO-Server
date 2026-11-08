//! `maps/<map>.nav` (Source navigation mesh, version 16 of CS:GO) to a small text graph for plugins:
//!
//! ```text
//! #csls-nav 1
//! P <place id> <name>
//! A <id> <nw x y z> <se x y z> <ne z> <sw z> <place id> <attributes>
//! C <id> <connected id>...        (one line per area with exits)
//! H <area id> <x y z> <flags>     (hiding spot)
//! ```
//!
//! Only what a path finder and a "where do I stand" plugin need; ladders and the visibility lists are skipped.

use anyhow::{bail, Context};
use std::fmt::Write;

#[derive(Debug, PartialEq)]
pub struct Area {
    pub id: u32,
    pub attrs: u32,
    pub nw: [f32; 3],
    pub se: [f32; 3],
    pub ne_z: f32,
    pub sw_z: f32,
    pub place: u16,
    pub exits: Vec<u32>,
    pub spots: Vec<([f32; 3], u8)>,
}

#[derive(Debug, PartialEq)]
pub struct Nav {
    pub places: Vec<String>,
    pub areas: Vec<Area>,
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> crate::Result<&[u8]> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.b.len()).context("the nav file ends too soon")?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u8(&mut self) -> crate::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> crate::Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> crate::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> crate::Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn skip(&mut self, n: usize) -> crate::Result<()> {
        self.take(n).map(|_| ())
    }
}

/// Reads the areas of a v16 nav file. Anything else is refused: a wrong guess about the layout would read garbage.
pub fn parse(b: &[u8]) -> crate::Result<Nav> {
    let mut r = Reader { b, at: 0 };
    if r.u32()? != 0xFEED_FACE {
        bail!("not a nav file");
    }
    let version = r.u32()?;
    if version != 16 {
        bail!("nav version {version} is not supported (16 is)");
    }
    r.skip(4 + 4 + 1)?; // sub version, size of the map it was built for, "analyzed"
    let n_places = r.u16()?;
    let mut places = Vec::with_capacity(n_places as usize);
    for _ in 0..n_places {
        let len = r.u16()? as usize;
        let raw = r.take(len)?;
        places.push(String::from_utf8_lossy(raw.split(|c| *c == 0).next().unwrap_or(&[])).into_owned());
    }
    r.skip(1)?; // has unnamed areas
    let n_areas = r.u32()?;
    if n_areas == 0 || n_areas > 200_000 {
        bail!("implausible number of nav areas: {n_areas}");
    }
    let mut areas = Vec::with_capacity(n_areas as usize);
    for _ in 0..n_areas {
        let id = r.u32()?;
        let attrs = r.u32()?;
        let mut v = [0f32; 6];
        for x in &mut v {
            *x = r.f32()?;
        }
        let (ne_z, sw_z) = (r.f32()?, r.f32()?);
        let mut exits = Vec::new();
        for _ in 0..4 {
            let n = r.u32()?;
            if n > 1000 {
                bail!("implausible number of connections: {n}");
            }
            for _ in 0..n {
                exits.push(r.u32()?);
            }
        }
        let mut spots = Vec::new();
        for _ in 0..r.u8()? {
            r.skip(4)?; // id
            let p = [r.f32()?, r.f32()?, r.f32()?];
            spots.push((p, r.u8()?));
        }
        for _ in 0..r.u32()? {
            // encounter paths: from area and direction, to area and direction, then the spots passed
            r.skip(4 + 1 + 4 + 1)?;
            let n = r.u8()? as usize;
            r.skip(n * 5)?;
        }
        let place = r.u16()?;
        for _ in 0..2 {
            let n = r.u32()? as usize;
            r.skip(n.checked_mul(4).context("overflow")?)?; // ladder connections
        }
        r.skip(2 * 4 + 4 * 4)?; // earliest occupy times, light intensity
        let n = r.u32()? as usize;
        r.skip(n.checked_mul(5).context("overflow")?)?; // visible areas: id + attributes
        r.skip(4)?; // inherit visibility from
        let n = r.u8()? as usize;
        r.skip(n * 14)?; // v16 trailer: a count and 14-byte records (found by reading the real files)
        areas.push(Area { id, attrs, nw: [v[0], v[1], v[2]], se: [v[3], v[4], v[5]], ne_z, sw_z, place, exits, spots });
    }
    // a real mesh only connects areas that exist; if the layout was misread this is where it shows
    let mut ids: Vec<u32> = areas.iter().map(|a| a.id).collect();
    ids.sort_unstable();
    if let Some(bad) = areas.iter().flat_map(|a| &a.exits).find(|e| ids.binary_search(e).is_err()) {
        bail!("nav connection to unknown area {bad}: the file is not laid out as expected");
    }
    Ok(Nav { places, areas })
}

fn f(x: f32) -> String {
    format!("{x:.1}")
}

pub fn render(n: &Nav) -> String {
    let mut o = String::from("#csls-nav 1\n");
    for (i, p) in n.places.iter().enumerate() {
        let _ = writeln!(o, "P {} {}", i + 1, p.replace(char::is_whitespace, "_"));
    }
    for a in &n.areas {
        let _ = writeln!(
            o,
            "A {} {} {} {} {} {} {} {} {} {} {}",
            a.id,
            f(a.nw[0]),
            f(a.nw[1]),
            f(a.nw[2]),
            f(a.se[0]),
            f(a.se[1]),
            f(a.se[2]),
            f(a.ne_z),
            f(a.sw_z),
            a.place,
            a.attrs
        );
    }
    for a in n.areas.iter().filter(|a| !a.exits.is_empty()) {
        let list: Vec<String> = a.exits.iter().map(u32::to_string).collect();
        let _ = writeln!(o, "C {} {}", a.id, list.join(" "));
    }
    for a in &n.areas {
        for (p, flags) in &a.spots {
            let _ = writeln!(o, "H {} {} {} {} {flags}", a.id, f(p[0]), f(p[1]), f(p[2]));
        }
    }
    o
}

pub fn export(b: &[u8]) -> crate::Result<String> {
    Ok(render(&parse(b)?))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A nav file with two connected areas, one place and one hiding spot.
    pub fn tiny() -> Vec<u8> {
        let mut b = Vec::new();
        let u32s = |b: &mut Vec<u8>, v: u32| b.extend(v.to_le_bytes());
        let f32s = |b: &mut Vec<u8>, v: f32| b.extend(v.to_le_bytes());
        u32s(&mut b, 0xFEED_FACE);
        u32s(&mut b, 16);
        u32s(&mut b, 1);
        u32s(&mut b, 1234);
        b.push(1);
        b.extend(1u16.to_le_bytes());
        b.extend(10u16.to_le_bytes());
        b.extend(b"Bombsite\0\0".iter());
        b.push(0);
        u32s(&mut b, 2);
        for (id, to) in [(10u32, 20u32), (20, 10)] {
            u32s(&mut b, id);
            u32s(&mut b, 0);
            for v in [0.0, 0.0, 0.0, 64.0, 64.0, 0.0, 0.0, 0.0] {
                f32s(&mut b, v);
            }
            u32s(&mut b, 1);
            u32s(&mut b, to);
            for _ in 0..3 {
                u32s(&mut b, 0);
            }
            if id == 10 {
                b.push(1);
                u32s(&mut b, 7);
                for v in [32.0, 32.0, 0.0] {
                    f32s(&mut b, v);
                }
                b.push(1);
            } else {
                b.push(0);
            }
            u32s(&mut b, 1); // one encounter path with two spots
            u32s(&mut b, id);
            b.push(0);
            u32s(&mut b, to);
            b.push(2);
            b.push(2);
            b.extend([0u8; 10]);
            b.extend(1u16.to_le_bytes());
            u32s(&mut b, 0);
            u32s(&mut b, 0);
            for _ in 0..6 {
                f32s(&mut b, 0.0);
            }
            u32s(&mut b, 0);
            u32s(&mut b, 0);
            b.push(0);
        }
        b
    }

    #[test]
    fn reads_a_small_mesh() {
        let n = parse(&tiny()).unwrap();
        assert_eq!(n.places, vec!["Bombsite".to_string()]);
        assert_eq!(n.areas.len(), 2);
        assert_eq!(n.areas[0].exits, vec![20]);
        assert_eq!(n.areas[0].spots.len(), 1);
        let text = render(&n);
        assert!(text.contains("P 1 Bombsite") && text.contains("C 10 20") && text.contains("H 10 32.0 32.0 0.0 1"));
    }

    #[test]
    fn refuses_the_wrong_file() {
        assert!(parse(b"nope").is_err());
        let mut v = tiny();
        v[4] = 17;
        assert!(parse(&v).is_err(), "other versions are not guessed");
        let v = tiny();
        assert!(parse(&v[..v.len() - 3]).is_err(), "truncated");
    }

    /// The nav files of the installed game: every one must parse and connect only to existing areas.
    #[test]
    fn real_maps_parse() {
        let Some(dir) = dirs_maps() else { return };
        let mut n = 0;
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            if e.path().extension().is_some_and(|x| x == "nav") {
                let nav = parse(&std::fs::read(e.path()).unwrap()).unwrap_or_else(|er| panic!("{}: {er}", e.path().display()));
                assert!(!nav.areas.is_empty(), "{}", e.path().display());
                n += 1;
            }
        }
        eprintln!("{n} real nav files parsed");
    }

    fn dirs_maps() -> Option<std::path::PathBuf> {
        let p = std::path::PathBuf::from(std::env::var_os("HOME")?).join(".local/share/csgo-legacy-server/server/csgo/maps");
        p.is_dir().then_some(p)
    }
}
