#![allow(dead_code)] // API pequena e completa, nem tudo e usado ainda.

// Escritor de JSON minimo (sem dependencias externas -> build offline garantido).

pub struct J {
    pub s: String,
    first: bool,
}

impl J {
    pub fn obj() -> Self {
        J {
            s: String::from("{"),
            first: true,
        }
    }

    pub fn arr() -> Self {
        J {
            s: String::from("["),
            first: true,
        }
    }

    fn sep(&mut self, key: Option<&str>) {
        if !self.first {
            self.s.push(',');
        }
        self.first = false;
        if let Some(k) = key {
            self.s.push('"');
            self.s.push_str(&esc(k));
            self.s.push_str("\":");
        }
    }

    pub fn kv(mut self, k: &str, v: J) -> Self {
        self.sep(Some(k));
        self.s.push_str(&v.s);
        self
    }

    pub fn kstr(self, k: &str, v: &str) -> Self {
        self.kv(k, J::str(v))
    }

    pub fn kbool(self, k: &str, v: bool) -> Self {
        self.kv(k, J::boolean(v))
    }

    pub fn knum(self, k: &str, v: f64) -> Self {
        self.kv(k, J::num(v))
    }

    pub fn push(mut self, v: J) -> Self {
        self.sep(None);
        self.s.push_str(&v.s);
        self
    }

    pub fn push_str(mut self, v: &str) -> Self {
        self.sep(None);
        self.s.push_str(&J::str(v).s);
        self
    }

    pub fn raw(v: String) -> J {
        J { s: v, first: true }
    }

    pub fn str(v: &str) -> J {
        J {
            s: format!("\"{}\"", esc(v)),
            first: true,
        }
    }

    pub fn num(v: f64) -> J {
        let s = if v.fract() == 0.0 {
            format!("{}", v as i64)
        } else {
            format!("{v:.3}")
        };
        J { s, first: true }
    }

    pub fn boolean(v: bool) -> J {
        J {
            s: v.to_string(),
            first: true,
        }
    }

    pub fn null() -> J {
        J {
            s: "null".into(),
            first: true,
        }
    }

    pub fn strs<I: IntoIterator<Item = S>, S: AsRef<str>>(items: I) -> J {
        let mut a = J::arr();
        for i in items {
            a = a.push_str(i.as_ref());
        }
        a
    }

    /// Fecha o objeto/array e devolve como valor JSON aninhado.
    pub fn end(mut self) -> J {
        self.s.push(if self.s.starts_with('[') { ']' } else { '}' });
        J {
            s: self.s,
            first: true,
        }
    }

    /// Fecha e devolve o texto final.
    pub fn done(self) -> String {
        self.end().s
    }
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}
