//! Address expressions, Cheat-Engine style:
//!
//! * `0x7ffd1234` / `7ffd1234` — numbers are hex; `#100` is decimal
//! * `Game.exe+0x1A2B30` — module base plus offset (`"name with spaces.dll"` may be quoted)
//! * `[[Game.exe+1A2B30]+18]+40` — `[...]` dereferences a pointer
//!
//! Pointer paths written this way survive game restarts, unlike raw addresses.

use crate::{Error, Result};

pub trait Resolver {
    fn module_base(&self, name: &str) -> Option<u64>;
    fn read_ptr(&self, addr: u64) -> Option<u64>;
}

pub fn eval(expr: &str, r: &dyn Resolver) -> Result<u64> {
    let toks = lex(expr)?;
    let mut p = Parser { toks, pos: 0, depth: 0, r };
    let v = p.sum()?;
    if p.pos != p.toks.len() {
        return Err(Error::invalid(format!("unexpected trailing input in '{expr}'")));
    }
    Ok(v)
}

/// True if the expression is a plain number (no module or dereference).
pub fn is_absolute(expr: &str) -> bool {
    crate::parse_addr(expr).is_some()
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(u64),
    Name(String),
    Plus,
    Minus,
    LBrack,
    RBrack,
    LParen,
    RParen,
}

fn lex(s: &str) -> Result<Vec<Tok>> {
    let mut out = Vec::new();
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        match c {
            ' ' | '\t' => i += 1,
            '+' => {
                out.push(Tok::Plus);
                i += 1
            }
            '-' => {
                out.push(Tok::Minus);
                i += 1
            }
            '[' => {
                out.push(Tok::LBrack);
                i += 1
            }
            ']' => {
                out.push(Tok::RBrack);
                i += 1
            }
            '(' => {
                out.push(Tok::LParen);
                i += 1
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1
            }
            '"' => {
                let end = cs[i + 1..].iter().position(|c| *c == '"').ok_or_else(|| Error::invalid("unterminated quote"))?;
                out.push(Tok::Name(cs[i + 1..i + 1 + end].iter().collect()));
                i += end + 2;
            }
            '#' => {
                let start = i + 1;
                i = start;
                while i < cs.len() && cs[i].is_ascii_digit() {
                    i += 1;
                }
                let t: String = cs[start..i].iter().collect();
                out.push(Tok::Num(t.parse().map_err(|_| Error::invalid(format!("bad decimal '#{t}'")))?));
            }
            _ if c.is_alphanumeric() || c == '_' || c == '.' => {
                let start = i;
                while i < cs.len() && (cs[i].is_alphanumeric() || matches!(cs[i], '_' | '.' | '$' | '@')) {
                    i += 1;
                }
                let word: String = cs[start..i].iter().collect();
                match crate::parse_addr(&word) {
                    Some(v) if !word.contains('.') => out.push(Tok::Num(v)),
                    _ => out.push(Tok::Name(word)),
                }
            }
            _ => return Err(Error::invalid(format!("unexpected '{c}' in address expression"))),
        }
    }
    Ok(out)
}

/// Parentheses and dereferences nest this deep at most: the parser recurses, and a deeply
/// nested expression must be an error, not a stack overflow that takes the engine down.
const MAX_DEPTH: usize = 64;

struct Parser<'a> {
    toks: Vec<Tok>,
    pos: usize,
    depth: usize,
    r: &'a dyn Resolver,
}

impl Parser<'_> {
    fn sum(&mut self) -> Result<u64> {
        let mut v = self.atom()?;
        while let Some(t) = self.toks.get(self.pos).cloned() {
            match t {
                Tok::Plus => {
                    self.pos += 1;
                    v = v.wrapping_add(self.atom()?)
                }
                Tok::Minus => {
                    self.pos += 1;
                    v = v.wrapping_sub(self.atom()?)
                }
                _ => break,
            }
        }
        Ok(v)
    }

    fn atom(&mut self) -> Result<u64> {
        let t = self.toks.get(self.pos).cloned().ok_or_else(|| Error::invalid("incomplete address expression"))?;
        self.pos += 1;
        match t {
            Tok::Num(v) => Ok(v),
            Tok::Name(n) => self.r.module_base(&n).ok_or_else(|| Error::new("unknown_module", format!("no module named '{n}' in target"))),
            Tok::LParen => {
                self.enter()?;
                let v = self.sum()?;
                self.depth -= 1;
                self.expect(Tok::RParen)?;
                Ok(v)
            }
            Tok::LBrack => {
                self.enter()?;
                let a = self.sum()?;
                self.depth -= 1;
                self.expect(Tok::RBrack)?;
                self.r.read_ptr(a).ok_or_else(|| Error::new("bad_pointer", format!("cannot dereference {}", crate::hex(a))))
            }
            other => Err(Error::invalid(format!("unexpected {other:?}"))),
        }
    }

    fn enter(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(Error::invalid("address expression is nested too deeply"));
        }
        Ok(())
    }

    fn expect(&mut self, t: Tok) -> Result<()> {
        if self.toks.get(self.pos) == Some(&t) {
            self.pos += 1;
            Ok(())
        } else {
            Err(Error::invalid(format!("expected {t:?}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    impl Resolver for Fake {
        fn module_base(&self, n: &str) -> Option<u64> {
            (n.eq_ignore_ascii_case("game.exe")).then_some(0x1000)
        }
        fn read_ptr(&self, a: u64) -> Option<u64> {
            match a {
                0x1010 => Some(0x5000),
                0x5018 => Some(0x9000),
                _ => None,
            }
        }
    }

    #[test]
    fn evaluates() {
        assert_eq!(eval("0x10+#16", &Fake).unwrap(), 0x20);
        assert_eq!(eval("Game.exe+10", &Fake).unwrap(), 0x1010);
        assert_eq!(eval("[[Game.exe+10]+18]+40", &Fake).unwrap(), 0x9040);
        assert_eq!(eval("\"game.exe\" - 0x10", &Fake).unwrap(), 0xff0);
        assert!(eval("[0x1]", &Fake).is_err());
        assert!(eval("nosuch.dll+4", &Fake).is_err());
        let deep = format!("{}0{}", "(".repeat(100_000), ")".repeat(100_000));
        assert!(eval(&deep, &Fake).is_err());
    }
}
