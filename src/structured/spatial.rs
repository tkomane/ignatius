use super::{Error, Input, Parsed, Tree};

const HAS_Z: u32 = 0x8000_0000;
const HAS_M: u32 = 0x4000_0000;
const HAS_SRID: u32 = 0x2000_0000;

pub(super) fn parse(tree: &mut Tree<'_>, raw: &str) -> Parsed<()> {
    let hex = raw.strip_prefix("\\x").unwrap_or(raw);
    if !hex.len().is_multiple_of(2) {
        return Err(Error::Invalid);
    }
    let bytes = hex
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| Ok(hex_digit(chunk[0])? << 4 | hex_digit(chunk[1])?))
        .collect::<Parsed<Vec<_>>>()?;
    let mut reader = Wkb {
        bytes: &bytes,
        next: 0,
    };
    reader.geometry(tree, None, tree.kind.label().to_owned())?;
    if reader.next != bytes.len() {
        return Err(Error::Invalid);
    }
    tree.set_text(0, 0..raw.len(), Some(Input::Whole));
    Ok(())
}

fn hex_digit(byte: u8) -> Parsed<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(Error::Invalid),
    }
}

#[derive(Clone, Copy)]
enum Order {
    Big,
    Little,
}

struct Wkb<'a> {
    bytes: &'a [u8],
    next: usize,
}

impl Wkb<'_> {
    fn bytes<const N: usize>(&mut self) -> Parsed<[u8; N]> {
        let end = self.next + N;
        let data = self
            .bytes
            .get(self.next..end)
            .ok_or(Error::Invalid)?
            .try_into()
            .map_err(|_| Error::Invalid)?;
        self.next = end;
        Ok(data)
    }

    fn u32(&mut self, order: Order) -> Parsed<u32> {
        let bytes = self.bytes()?;
        Ok(match order {
            Order::Big => u32::from_be_bytes(bytes),
            Order::Little => u32::from_le_bytes(bytes),
        })
    }

    fn f64(&mut self, order: Order) -> Parsed<f64> {
        let bytes = self.bytes()?;
        Ok(match order {
            Order::Big => f64::from_be_bytes(bytes),
            Order::Little => f64::from_le_bytes(bytes),
        })
    }

    fn geometry(
        &mut self,
        tree: &mut Tree<'_>,
        parent: Option<usize>,
        label: String,
    ) -> Parsed<()> {
        let node = tree.add(parent, label)?;
        let order = match self.bytes::<1>()?[0] {
            0 => Order::Big,
            1 => Order::Little,
            _ => return Err(Error::Invalid),
        };
        let word = self.u32(order)?;
        let kind = word & !(HAS_Z | HAS_M | HAS_SRID);
        let (kind, z, m) = match kind / 1000 {
            0 => (kind, word & HAS_Z != 0, word & HAS_M != 0),
            1 => (kind % 1000, true, false),
            2 => (kind % 1000, false, true),
            3 => (kind % 1000, true, true),
            _ => return Err(Error::Unsupported),
        };
        let axes = match (z, m) {
            (false, false) => "XY",
            (true, false) => "XYZ",
            (false, true) => "XYM",
            (true, true) => "XYZM",
        };
        let name = match kind {
            1 => "Point",
            2 => "LineString",
            3 => "Polygon",
            4 => "MultiPoint",
            5 => "MultiLineString",
            6 => "MultiPolygon",
            7 => "GeometryCollection",
            8 => "CircularString",
            9 => "CompoundCurve",
            10 => "CurvePolygon",
            11 => "MultiCurve",
            12 => "MultiSurface",
            15 => "PolyhedralSurface",
            16 => "TIN",
            17 => "Triangle",
            _ => return Err(Error::Unsupported),
        };
        let summary = if word & HAS_SRID == 0 {
            format!("{name} {axes}")
        } else {
            let srid = self.u32(order)?;
            format!("{name} {axes}, SRID {srid}")
        };
        tree.summarize(node, summary);
        match kind {
            1 => self.coordinate(tree, node, order, axes, "Coordinate".to_owned()),
            2 | 8 => self.points(tree, node, order, axes),
            3 | 17 => {
                let count = self.u32(order)?;
                for index in 0..count {
                    let ring = tree.add(Some(node), format!("Ring {}", index + 1))?;
                    self.points(tree, ring, order, axes)?;
                }
                Ok(())
            }
            _ => {
                let count = self.u32(order)?;
                for index in 0..count {
                    self.geometry(tree, Some(node), format!("Part {}", index + 1))?;
                }
                Ok(())
            }
        }
    }

    fn points(
        &mut self,
        tree: &mut Tree<'_>,
        parent: usize,
        order: Order,
        axes: &str,
    ) -> Parsed<()> {
        let count = self.u32(order)?;
        for index in 0..count {
            self.coordinate(
                tree,
                parent,
                order,
                axes,
                format!("Coordinate {}", index + 1),
            )?;
        }
        Ok(())
    }

    fn coordinate(
        &mut self,
        tree: &mut Tree<'_>,
        parent: usize,
        order: Order,
        axes: &str,
        label: String,
    ) -> Parsed<()> {
        let node = tree.add(Some(parent), label)?;
        let mut values = Vec::new();
        let mut empty = true;
        for axis in axes.chars() {
            let value = self.f64(order)?;
            empty &= value.is_nan();
            values.push(format!("{axis}={value:?}"));
        }
        tree.summarize(
            node,
            if empty {
                "empty point".into()
            } else {
                values.join(", ")
            },
        );
        Ok(())
    }
}
