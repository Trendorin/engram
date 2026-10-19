//! x86/x86-64 disassembly (iced-x86, Intel syntax).

use crate::maps::{symbolize, Module};
use crate::memory::ProcMem;
use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, IntelFormatter};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Insn {
    pub address: String,
    pub location: String,
    pub bytes: String,
    pub text: String,
}

fn fmt_insn(f: &mut IntelFormatter, ins: &Instruction, bytes: &[u8], mods: &[Module]) -> Insn {
    let mut text = String::new();
    f.format(ins, &mut text);
    Insn { address: crate::hex(ins.ip()), location: symbolize(mods, ins.ip()), bytes: crate::value::hex_bytes(bytes), text }
}

pub fn disassemble(mem: &ProcMem, addr: u64, count: usize, bitness: u32, mods: &[Module]) -> crate::Result<Vec<Insn>> {
    let want = (count * 15).clamp(16, 8192);
    let mut buf = vec![0u8; want];
    let n = mem.read(addr, &mut buf)?;
    if n == 0 {
        return Err(crate::Error::new("unreadable", format!("{} is not readable", crate::hex(addr))));
    }
    buf.truncate(n);
    let mut dec = Decoder::with_ip(bitness, &buf, addr, DecoderOptions::NONE);
    let mut f = IntelFormatter::new();
    f.options_mut().set_hex_prefix("0x");
    f.options_mut().set_hex_suffix("");
    f.options_mut().set_uppercase_hex(false);
    let mut out = Vec::new();
    let mut ins = Instruction::default();
    while dec.can_decode() && out.len() < count {
        dec.decode_out(&mut ins);
        let off = (ins.ip() - addr) as usize;
        out.push(fmt_insn(&mut f, &ins, &buf[off..off + ins.len()], mods));
    }
    Ok(out)
}

/// Data breakpoints trap *after* the faulting instruction; find the instruction
/// that ends exactly at `rip`. Prefers candidates with a memory operand.
pub fn instruction_before(mem: &ProcMem, rip: u64, bitness: u32, mods: &[Module]) -> Option<Insn> {
    let start = rip.checked_sub(15)?;
    let mut buf = [0u8; 15];
    mem.read_exact(start, &mut buf).ok()?;
    let mut f = IntelFormatter::new();
    f.options_mut().set_hex_prefix("0x");
    f.options_mut().set_hex_suffix("");
    f.options_mut().set_uppercase_hex(false);
    let mut best: Option<(bool, usize, Instruction)> = None;
    for back in 1..=15usize {
        let off = 15 - back;
        let mut dec = Decoder::with_ip(bitness, &buf[off..], start + off as u64, DecoderOptions::NONE);
        let ins = dec.decode();
        if ins.is_invalid() || ins.len() != back {
            continue;
        }
        let has_mem = (0..ins.op_count()).any(|i| ins.op_kind(i) == iced_x86::OpKind::Memory);
        // Prefer memory-operand instructions, then the shortest plausible match.
        if best.as_ref().is_none_or(|(m, _, _)| has_mem && !m) {
            best = Some((has_mem, back, ins));
        }
    }
    let (_, back, ins) = best?;
    Some(fmt_insn(&mut f, &ins, &buf[15 - back..], mods))
}
