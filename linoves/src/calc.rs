//! Целочисленный калькулятор: + - * / % и скобки.
//! Переполнение и деление на ноль — это ошибки, а не падение ядра;
//! глубина скобок ограничена, чтобы не переполнить стек.

const MAX_DEPTH: u32 = 64;

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
    depth: u32,
}

pub fn eval(expr: &str) -> Result<i64, &'static str> {
    let mut p = Parser { s: expr.as_bytes(), pos: 0, depth: 0 };
    let v = p.expr()?;
    p.skip_ws();
    if p.pos != p.s.len() {
        return Err("unexpected character");
    }
    Ok(v)
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while self.pos < self.s.len() && self.s[self.pos] == b' ' {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.s.get(self.pos).copied()
    }

    fn expr(&mut self) -> Result<i64, &'static str> {
        let mut v = self.term()?;
        while let Some(op @ (b'+' | b'-')) = self.peek() {
            self.pos += 1;
            let r = self.term()?;
            v = if op == b'+' { v.checked_add(r) } else { v.checked_sub(r) }.ok_or("overflow")?;
        }
        Ok(v)
    }

    fn term(&mut self) -> Result<i64, &'static str> {
        let mut v = self.factor()?;
        while let Some(op @ (b'*' | b'/' | b'%')) = self.peek() {
            self.pos += 1;
            let r = self.factor()?;
            v = match op {
                b'*' => v.checked_mul(r).ok_or("overflow")?,
                _ if r == 0 => return Err("division by zero"),
                b'/' => v.checked_div(r).ok_or("overflow")?,
                _ => v.checked_rem(r).ok_or("overflow")?,
            };
        }
        Ok(v)
    }

    fn factor(&mut self) -> Result<i64, &'static str> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err("expression is nested too deeply");
        }
        let r = match self.peek() {
            Some(b'-') => {
                self.pos += 1;
                self.factor()?.checked_neg().ok_or("overflow")
            }
            Some(b'+') => {
                self.pos += 1;
                self.factor()
            }
            Some(b'(') => {
                self.pos += 1;
                let v = self.expr()?;
                if self.peek() != Some(b')') {
                    return Err("missing ')'");
                }
                self.pos += 1;
                Ok(v)
            }
            Some(b'0'..=b'9') => {
                let mut v: i64 = 0;
                while let Some(&c @ b'0'..=b'9') = self.s.get(self.pos) {
                    v = v
                        .checked_mul(10)
                        .and_then(|v| v.checked_add((c - b'0') as i64))
                        .ok_or("number too large")?;
                    self.pos += 1;
                }
                Ok(v)
            }
            Some(_) => Err("unexpected character"),
            None => Err("unexpected end of expression"),
        };
        self.depth -= 1;
        r
    }
}
