//! The original's Mouse Systems serial mouse, as `DGMS.DRV` reads it.
//!
//! A packet is a start byte `0x80 | buttons` (active low: bit 2 left, bit 1
//! middle, bit 0 right) and four signed deltas `dx1, dy1, dx2, dy2`, positive
//! y upward. The driver adds `d × 10` for `|d| ≤ 2` and `d × 60` otherwise to
//! an absolute position clamped to `0..=RANGE`. It treats any byte
//! `0x80..=0x87` as a packet start, so deltas stay within `-120..=127`.

pub const RANGE: i32 = 20480;
pub const LEFT: u8 = 4;
pub const MIDDLE: u8 = 2;
pub const RIGHT: u8 = 1;
const MIN_DELTA: i8 = -120;
const FINE: i32 = 10;
const COARSE: i32 = 60;

/// One packet moving by `(dx, dy)` with `buttons` (`LEFT | MIDDLE | RIGHT`)
/// held. The second delta pair is left at zero.
pub fn packet(dx: i8, dy: i8, buttons: u8) -> [u8; 5] {
    debug_assert!(
        dx >= MIN_DELTA && dy >= MIN_DELTA,
        "delta reads as a start byte"
    );
    [0x80 | (!buttons & 7), dx as u8, dy as u8, 0, 0]
}

/// The driver's position on one axis after one delta.
pub fn apply(position: i32, delta: i8) -> i32 {
    let d = i32::from(delta);
    let scale = if d.abs() <= 2 { FINE } else { COARSE };
    (position + d * scale).clamp(0, RANGE)
}

/// The delta that moves `position` toward `target` as far as possible
/// without overshooting. Both are multiples of 10.
fn step(position: i32, target: i32) -> i8 {
    let remaining = target - position;
    if remaining == 0 {
        0
    } else if remaining.abs() >= 3 * COARSE {
        (remaining / COARSE).clamp(i32::from(MIN_DELTA), 127) as i8
    } else {
        (remaining / FINE).clamp(-2, 2) as i8
    }
}

/// The guest driver's position and buttons, mirrored exactly from the
/// packets sent. The driver starts at `(0, 0)` with no buttons held.
pub struct Tracker {
    x: i32,
    y: i32,
    buttons: u8,
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            buttons: 0,
        }
    }

    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    /// Packets taking the pointer to `(x, y)`, rounded to the driver's
    /// 10-unit grid and clamped to its range, with `buttons` held. A button
    /// change alone still sends one packet.
    pub fn packets_to(&mut self, x: i32, y: i32, buttons: u8) -> Vec<[u8; 5]> {
        let grid = |v: i32| (v.clamp(0, RANGE) + 5) / 10 * 10;
        let target = (grid(x), grid(y));
        let mut pending = buttons != self.buttons;
        self.buttons = buttons;
        let mut out = Vec::new();
        while (self.x, self.y) != target || pending {
            let (dx, dy) = (step(self.x, target.0), step(self.y, target.1));
            self.x = apply(self.x, dx);
            self.y = apply(self.y, dy);
            out.push(packet(dx, dy, buttons));
            pending = false;
        }
        out
    }

    /// Packets driving the pointer into the driver's lower-left clamp, which
    /// makes the mirror exact again whatever the guest believed.
    pub fn pin(&mut self) -> Vec<[u8; 5]> {
        self.x = 0;
        self.y = 0;
        vec![packet(MIN_DELTA, MIN_DELTA, self.buttons); 3]
    }
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Packets to a guest that may stop reading, as AutoCAD does whenever its
/// driver is unhooked. A write that would block drops the rest of its batch
/// instead of stalling the caller, and marks the mirror stale; the next move
/// re-pins before heading for its target.
pub struct Link<W> {
    out: W,
    tracker: Tracker,
    stale: bool,
}

impl<W: std::io::Write> Link<W> {
    /// `out` should be non-blocking; a blocking writer simply never drops.
    pub fn new(out: W) -> Self {
        Self {
            out,
            tracker: Tracker::new(),
            stale: false,
        }
    }

    pub fn position(&self) -> (i32, i32) {
        self.tracker.position()
    }

    pub fn move_to(&mut self, x: i32, y: i32, buttons: u8) -> std::io::Result<()> {
        let mut packets = if self.stale {
            // The guest never saw the dropped batch's buttons either.
            self.tracker.buttons = buttons;
            self.tracker.pin()
        } else {
            Vec::new()
        };
        packets.extend(self.tracker.packets_to(x, y, buttons));
        self.send(&packets)
    }

    pub fn pin(&mut self) -> std::io::Result<()> {
        let packets = self.tracker.pin();
        self.send(&packets)
    }

    fn send(&mut self, packets: &[[u8; 5]]) -> std::io::Result<()> {
        match self.out.write_all(&packets.concat()) {
            Ok(()) => {
                self.stale = false;
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                self.stale = true;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

/// The CGA pixel under device position `(x, y)`, as measured from the
/// original's crosshair: columns exactly, rows within one.
pub fn pixel_for_device(x: i32, y: i32) -> (i32, i32) {
    ((x * 639 + RANGE / 2) / RANGE, 180 - y * 191 / RANGE)
}

/// A device position on the driver's grid whose crosshair lands on CGA
/// pixel `(column, row)`: the middle of that pixel's device interval.
pub fn device_for_pixel(column: usize, row: usize) -> (i32, i32) {
    let grid = |v: f64| ((v / 10.0).round() as i32 * 10).clamp(0, RANGE);
    let x = column as f64 * f64::from(RANGE) / 639.0;
    let y = (180.0 - row as f64 + 0.5) * f64::from(RANGE) / 191.0;
    (grid(x), grid(y))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replay(start: (i32, i32), packets: &[[u8; 5]]) -> (i32, i32) {
        packets.iter().fold(start, |(x, y), p| {
            (apply(x, p[1] as i8), apply(y, p[2] as i8))
        })
    }

    /// Accepts `room` more bytes, then reports `WouldBlock` like a full
    /// non-blocking socket.
    struct Choked {
        room: usize,
        written: Vec<u8>,
    }

    impl std::io::Write for Choked {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.room == 0 {
                return Err(std::io::ErrorKind::WouldBlock.into());
            }
            let n = bytes.len().min(self.room);
            self.room -= n;
            self.written.extend_from_slice(&bytes[..n]);
            Ok(n)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_full_socket_drops_the_batch_instead_of_blocking() {
        let mut link = Link::new(Choked {
            room: 7,
            written: Vec::new(),
        });
        link.move_to(15000, 15000, LEFT).unwrap();
        assert_eq!(link.out.written.len(), 7, "partial batch, no error");

        // Once the guest reads again, the next move re-pins first, so the
        // guest lands on the target whatever the partial batch did.
        link.out.room = usize::MAX;
        link.out.written.clear();
        link.move_to(5000, 6000, 0).unwrap();
        let packets: Vec<[u8; 5]> = link
            .out
            .written
            .chunks_exact(5)
            .map(|p| p.try_into().unwrap())
            .collect();
        assert_eq!(packets[0], packet(MIN_DELTA, MIN_DELTA, 0));
        for start in [(0, 0), (RANGE, RANGE), (15000, 15000)] {
            assert_eq!(replay(start, &packets), (5000, 6000));
        }
        assert_eq!(link.position(), (5000, 6000));

        // Healthy again: no further pins.
        link.out.written.clear();
        link.move_to(5010, 6000, 0).unwrap();
        assert_eq!(link.out.written, packet(1, 0, 0));
    }

    #[test]
    fn other_socket_errors_are_reported() {
        struct Broken;
        impl std::io::Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        assert!(Link::new(Broken).move_to(100, 100, 0).is_err());
    }

    #[test]
    fn packets_are_active_low_mouse_systems_frames() {
        assert_eq!(packet(1, -2, 0), [0x87, 0x01, 0xFE, 0, 0]);
        assert_eq!(packet(0, 0, LEFT), [0x83, 0, 0, 0, 0]);
        assert_eq!(packet(0, 0, LEFT | MIDDLE | RIGHT), [0x80, 0, 0, 0, 0]);
    }

    #[test]
    fn driver_arithmetic_accelerates_and_clamps() {
        assert_eq!(apply(0, 1), 10);
        assert_eq!(apply(100, -2), 80);
        assert_eq!(apply(0, 3), 180);
        assert_eq!(apply(1000, 100), 7000);
        assert_eq!(apply(50, -120), 0);
        assert_eq!(apply(20000, 127), RANGE);
    }

    #[test]
    fn tracker_reaches_every_grid_target_exactly() {
        let mut tracker = Tracker::new();
        for &(x, y) in &[
            (5000, 10000),
            (5010, 9990),
            (0, 0),
            (RANGE, RANGE),
            (1234, 17777),
        ] {
            let before = tracker.position();
            let packets = tracker.packets_to(x, y, 0);
            let want = ((x + 5) / 10 * 10, (y + 5) / 10 * 10);
            assert_eq!(tracker.position(), want, "tracker for {x},{y}");
            assert_eq!(replay(before, &packets), want, "driver for {x},{y}");
        }
    }

    #[test]
    fn deltas_never_look_like_packet_starts() {
        let mut tracker = Tracker::new();
        for target in (0..=RANGE).step_by(370) {
            for p in tracker.packets_to(target, RANGE - target, 0) {
                for &b in &p[1..] {
                    assert!(!(0x80..=0x87).contains(&b), "delta byte {b:#04x}");
                }
            }
        }
    }

    #[test]
    fn full_screen_jump_takes_few_packets() {
        let mut tracker = Tracker::new();
        assert!(tracker.packets_to(RANGE, RANGE, 0).len() <= 6);
        assert!(tracker.packets_to(0, 0, 0).len() <= 6);
    }

    #[test]
    fn button_changes_each_send_a_packet() {
        let mut tracker = Tracker::new();
        tracker.packets_to(5000, 5000, 0);
        let press = tracker.packets_to(5000, 5000, LEFT);
        let release = tracker.packets_to(5000, 5000, 0);
        assert_eq!(press, [packet(0, 0, LEFT)]);
        assert_eq!(release, [packet(0, 0, 0)]);
        assert!(tracker.packets_to(5000, 5000, 0).is_empty());
    }

    #[test]
    fn pin_makes_the_mirror_exact_from_any_state() {
        for start in [(0, 0), (20480, 20480), (7310, 150)] {
            let mut tracker = Tracker::new();
            tracker.packets_to(start.0, start.1, 0);
            let packets = tracker.pin();
            assert_eq!(tracker.position(), (0, 0));
            // Whatever the guest believed, the clamp brings it to (0, 0) too.
            assert_eq!(replay((RANGE, RANGE), &packets), (0, 0));
        }
    }

    #[test]
    fn measured_screen_mapping_matches_the_crosshair_samples() {
        for (x, column) in [
            (0, 0),
            (30, 1),
            (1000, 31),
            (5000, 156),
            (10000, 312),
            (17000, 530),
        ] {
            assert_eq!(pixel_for_device(x, 10000).0, column, "x {x}");
        }
        for (y, row) in [(5000, 134), (10000, 87), (15000, 41), (18500, 8)] {
            assert_eq!(pixel_for_device(9000, y).1, row, "y {y}");
        }
    }

    #[test]
    fn device_for_pixel_inverts_the_screen_mapping() {
        for column in 0..640 {
            for row in 8..=161 {
                let (x, y) = device_for_pixel(column, row);
                assert_eq!((x % 10, y % 10), (0, 0));
                assert_eq!(pixel_for_device(x, y), (column as i32, row as i32));
            }
        }
    }
}
