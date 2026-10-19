//! Fake game: a heap-allocated player reachable from a static pointer.
//! Control it by writing a command into the file given as argv[1]:
//! `hit` (health -7), `loot` (gold +25, arrows -1), `move` (x += 1.5), `quit`.

use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::Duration;

#[repr(C)]
struct Inventory {
    arrows: i32,
    potions: i32,
}

#[repr(C)]
struct Player {
    magic: u32,
    health: i32,
    max_health: i32,
    gold: i64,
    pos: [f32; 3],
    stamina: f32,
    inventory: Box<Inventory>,
    name: [u8; 16],
}

static PLAYER: AtomicPtr<Player> = AtomicPtr::new(std::ptr::null_mut());

fn main() {
    let ctl = std::env::args().nth(1).unwrap_or_else(|| "/tmp/engram-testbed.ctl".into());
    let mut name = [0u8; 16];
    name[..6].copy_from_slice(b"Hero01");
    let p = Box::new(Player {
        magic: 0x5041_4c59,
        health: 100,
        max_health: 100,
        gold: 1234,
        pos: [10.0, 0.0, -3.25],
        stamina: 75.5,
        inventory: Box::new(Inventory { arrows: 37, potions: 3 }),
        name,
    });
    PLAYER.store(Box::into_raw(p), Ordering::SeqCst);
    println!("testbed pid {} ctl {}", std::process::id(), ctl);
    loop {
        std::thread::sleep(Duration::from_millis(50));
        // SAFETY: single-threaded owner of the leaked player.
        let p = unsafe { &mut *PLAYER.load(Ordering::SeqCst) };
        let Ok(cmd) = std::fs::read_to_string(&ctl) else { continue };
        let _ = std::fs::remove_file(&ctl);
        match cmd.trim() {
            "hit" => p.health -= 7,
            "loot" => {
                p.gold += 25;
                p.inventory.arrows -= 1;
            }
            "move" => p.pos[0] += 1.5,
            "quit" => break,
            _ => {}
        }
        println!("health={} gold={} arrows={} x={}", p.health, p.gold, p.inventory.arrows, p.pos[0]);
    }
}
