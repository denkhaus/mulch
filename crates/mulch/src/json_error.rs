//! JavaScriptCore `JSON.parse` failure messages — the format core's
//! malformed-line reason law.
//!
//! The reference CLI parses JSONL lines and stdin/batch payloads with
//! JavaScriptCore's `JSON.parse` (Bun runtime), so a parse failure
//! carries JSC's message bytes, not serde's. Both readers render the
//! same law from here (mulch-943b, mulch-7c55).

// ---- The reference's JSON.parse error surface (mulch-943b) ----
//
// The reference CLI parses stdin/batch input with JavaScriptCore's
// JSON.parse (Bun runtime), so failures carry JSC's message bytes
// (`Failed to parse JSON from stdin: JSON Parse error: <message>`).
// The mirror below walks the input with JSC's probed model (Bun
// 1.4.0, the ml 0.10.7 runtime):
//
// - the LEXER owns number/string format errors and the single-quote and
//   unrecognized-character rejections; they preempt every parser context (`[1
//   @]` rejects the '@', not the missing ']'),
// - the PARSER owns the context messages (value position, object
//   key/colon/value steps, array element steps, trailing garbage),
// - words lex by identifier rules; `true`/`false`/`null` match as prefixes
//   first, so `trueX` leaves identifier `X` as trailing junk while `tru`
//   rejects the identifier itself,
// - an exponent without digits ends the number (`1e` is `1` plus identifier
//   `e`), a leading zero ends the integer part (`01` is `0` plus number `1`).

/// A JSC lexer token.
#[derive(Debug, PartialEq)]
enum Token {
    BraceOpen,
    BraceClose,
    BracketOpen,
    BracketClose,
    Comma,
    Colon,
    /// Recognized non-value punctuation: `(` `)` `;` `.` `=`.
    Punct(char),
    Str,
    Num,
    Word(String),
}

impl Token {
    /// The value-position rejection for every non-value token.
    fn unexpected_value(&self) -> String {
        match self {
            Token::BraceOpen | Token::BracketOpen | Token::Str | Token::Num | Token::Word(_) => {
                unreachable!("value tokens never reach the rejection")
            }
            Token::BraceClose => "Unexpected token '}'".into(),
            Token::BracketClose => "Unexpected token ']'".into(),
            Token::Comma => "Unexpected token ','".into(),
            Token::Colon => "Unexpected token ':'".into(),
            Token::Punct(c) => format!("Unexpected token '{c}'"),
        }
    }
}

/// The JavaScriptCore (Bun) JSON.parse failure message for `input`,
/// or `None` when JSC accepts it (serde may still reject lone \u
/// surrogates, which JSC decodes).
pub fn jsc_parse_error(input: &str) -> Option<String> {
    let mut parser = JscParser {
        chars: input.chars().collect(),
        pos:   0,
    };
    parser.parse().err()
}

struct JscParser {
    chars: Vec<char>,
    pos:   usize,
}

impl JscParser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn starts_with(&self, word: &str) -> bool {
        word.chars()
            .enumerate()
            .all(|(offset, c)| self.chars.get(self.pos + offset) == Some(&c))
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.pos += 1;
        }
    }

    /// Lexes the next token; `Ok(None)` marks end of input. Lexer
    /// errors preempt whatever the parser was about to report.
    fn lex(&mut self) -> Result<Option<Token>, String> {
        let Some(c) = self.peek() else {
            return Ok(None);
        };
        let token = match c {
            '{' => {
                self.pos += 1;
                Token::BraceOpen
            }
            '}' => {
                self.pos += 1;
                Token::BraceClose
            }
            '[' => {
                self.pos += 1;
                Token::BracketOpen
            }
            ']' => {
                self.pos += 1;
                Token::BracketClose
            }
            ',' => {
                self.pos += 1;
                Token::Comma
            }
            ':' => {
                self.pos += 1;
                Token::Colon
            }
            '(' | ')' | ';' | '.' | '=' => {
                self.pos += 1;
                Token::Punct(c)
            }
            '\'' => return Err("Single quotes (') are not allowed in JSON".into()),
            '"' => {
                self.lex_string()?;
                Token::Str
            }
            '-' | '0'..='9' => {
                self.lex_number()?;
                Token::Num
            }
            't' if self.starts_with("true") => {
                self.pos += 4;
                Token::Word("true".into())
            }
            'f' if self.starts_with("false") => {
                self.pos += 5;
                Token::Word("false".into())
            }
            'n' if self.starts_with("null") => {
                self.pos += 4;
                Token::Word("null".into())
            }
            'A'..='Z' | 'a'..='z' | '_' | '$' => Token::Word(self.lex_word()),
            _ => return Err(format!("Unrecognized token '{c}'")),
        };
        Ok(Some(token))
    }

    fn lex_word(&mut self) -> String {
        let start = self.pos;
        while matches!(
            self.peek(),
            Some('A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '$')
        ) {
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect()
    }

    /// Number per the JSON grammar with JSC's quirks: a digit must
    /// follow the sign (`Invalid number`) and the decimal point
    /// (`Invalid digits after decimal point`); an exponent without
    /// digits ends the token instead of failing.
    fn lex_number(&mut self) -> Result<(), String> {
        if self.peek() == Some('-') {
            self.pos += 1;
            if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
                return Err("Invalid number".into());
            }
        }
        if self.peek() == Some('0') {
            // a leading zero ends the integer part
            self.pos += 1;
        } else {
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if self.peek() == Some('.') {
            self.pos += 1;
            if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
                return Err("Invalid digits after decimal point".into());
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            let mut look = self.pos + 1;
            if matches!(self.chars.get(look), Some('+' | '-')) {
                look += 1;
            }
            if self.chars.get(look).is_some_and(char::is_ascii_digit) {
                self.pos = look;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
        }
        Ok(())
    }

    fn lex_string(&mut self) -> Result<(), String> {
        self.pos += 1; // opening quote
        loop {
            let Some(c) = self.peek() else {
                return Err("Unterminated string".into());
            };
            match c {
                '"' => {
                    self.pos += 1;
                    return Ok(());
                }
                '\\' => {
                    self.pos += 1;
                    let Some(escape) = self.peek() else {
                        return Err("Unterminated string".into());
                    };
                    match escape {
                        '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' => self.pos += 1,
                        'u' => {
                            self.pos += 1;
                            for _ in 0..4 {
                                match self.peek() {
                                    Some(hex) if hex.is_ascii_hexdigit() => self.pos += 1,
                                    Some(_) => {
                                        return Err("\\u must be followed by 4 hex digits".into());
                                    }
                                    None => return Err("Unterminated string".into()),
                                }
                            }
                        }
                        other => return Err(format!("Invalid escape character {other}")),
                    }
                }
                // a raw control character never terminates legally:
                // the string stays unterminated
                c if (c as u32) < 0x20 => return Err("Unterminated string".into()),
                _ => self.pos += 1,
            }
        }
    }

    /// Whole-input parse: one value, then only whitespace.
    fn parse(&mut self) -> Result<(), String> {
        self.skip_ws();
        self.value()?;
        self.skip_ws();
        if self.lex()?.is_some() {
            return Err("Unable to parse JSON string".into());
        }
        Ok(())
    }

    /// One value at the current position (leading whitespace
    /// skipped by the caller where the grammar demands it).
    fn value(&mut self) -> Result<(), String> {
        match self.lex()? {
            None => Err("Unexpected EOF".into()),
            Some(Token::BraceOpen) => self.object_body(),
            Some(Token::BracketOpen) => self.array_body(),
            Some(Token::Str | Token::Num) => Ok(()),
            Some(Token::Word(word)) => match word.as_str() {
                "true" | "false" | "null" => Ok(()),
                _ => Err(format!("Unexpected identifier \"{word}\"")),
            },
            Some(other) => Err(other.unexpected_value()),
        }
    }

    fn array_body(&mut self) -> Result<(), String> {
        self.skip_ws();
        match self.peek() {
            None => return Err("Unexpected EOF".into()),
            Some(']') => {
                self.pos += 1;
                return Ok(());
            }
            _ => {}
        }
        loop {
            self.value()?;
            self.skip_ws();
            match self.lex()? {
                Some(Token::BracketClose) => return Ok(()),
                Some(Token::Comma) => {
                    self.skip_ws();
                    match self.peek() {
                        None => return Err("Unexpected EOF".into()),
                        Some(']') => {
                            return Err("Unexpected comma at the end of array expression".into());
                        }
                        _ => {}
                    }
                }
                None | Some(_) => return Err("Expected ']'".into()),
            }
        }
    }

    fn object_body(&mut self) -> Result<(), String> {
        self.skip_ws();
        if self.peek() == Some('}') {
            self.pos += 1;
            return Ok(());
        }
        // the first key blames the brace; later ones (after a comma)
        // blame the property name
        self.property(false)?;
        loop {
            self.skip_ws();
            match self.lex()? {
                Some(Token::BraceClose) => return Ok(()),
                Some(Token::Comma) => self.property(true)?,
                None | Some(_) => return Err("Expected '}'".into()),
            }
        }
    }

    /// One `"key": value` pair.
    fn property(&mut self, after_comma: bool) -> Result<(), String> {
        self.skip_ws();
        match self.lex()? {
            Some(Token::Str) => {}
            None | Some(_) if after_comma => {
                return Err("Property name must be a string literal".into());
            }
            None | Some(_) => return Err("Expected '}'".into()),
        }
        self.skip_ws();
        if !matches!(self.lex()?, Some(Token::Colon)) {
            return Err("Expected ':' before value in object property definition".into());
        }
        self.skip_ws();
        self.value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins probed against Bun 1.4.0 (ml 0.10.7's runtime): every
    /// row is JSON.parse's exact message for the input.
    #[test]
    fn jsc_parse_error_pins_bun_messages() {
        let cases: &[(&str, &str)] = &[
            ("", "Unexpected EOF"),
            ("   ", "Unexpected EOF"),
            ("not json at all", "Unexpected identifier \"not\""),
            ("nope{", "Unexpected identifier \"nope\""),
            ("tru", "Unexpected identifier \"tru\""),
            ("nul", "Unexpected identifier \"nul\""),
            ("NaN", "Unexpected identifier \"NaN\""),
            ("$x", "Unexpected identifier \"$x\""),
            ("e5", "Unexpected identifier \"e5\""),
            ("trueX", "Unable to parse JSON string"),
            ("true false", "Unable to parse JSON string"),
            ("nullnull", "Unable to parse JSON string"),
            ("01", "Unable to parse JSON string"),
            ("00", "Unable to parse JSON string"),
            ("1-", "Invalid number"),
            ("1--", "Invalid number"),
            ("-", "Invalid number"),
            ("-.5", "Invalid number"),
            ("-x", "Invalid number"),
            ("1.", "Invalid digits after decimal point"),
            ("1.e5", "Invalid digits after decimal point"),
            ("1e", "Unable to parse JSON string"),
            // probed: `JSON.parse("1.5-2")` rejects the trailing
            // `-2` (Bun 1.4.0), it is not a valid number continuation
            ("1.5-2", "Unable to parse JSON string"),
            ("1e+", "Unable to parse JSON string"),
            ("1.2.3", "Unable to parse JSON string"),
            ("1.2e", "Unable to parse JSON string"),
            ("@ ", "Unrecognized token '@'"),
            ("+1", "Unrecognized token '+'"),
            (".5", "Unexpected token '.'"),
            ("=", "Unexpected token '='"),
            (";", "Unexpected token ';'"),
            (",", "Unexpected token ','"),
            ("}", "Unexpected token '}'"),
            ("]", "Unexpected token ']'"),
            ("{'a':1}", "Single quotes (') are not allowed in JSON"),
            ("[1 'x']", "Single quotes (') are not allowed in JSON"),
            ("\"abc", "Unterminated string"),
            ("\"a\\", "Unterminated string"),
            ("\"a\\u12\"", "\\u must be followed by 4 hex digits"),
            ("\"ab\\qc\"", "Invalid escape character q"),
            ("[", "Unexpected EOF"),
            ("[1,", "Unexpected EOF"),
            ("[,]", "Unexpected token ','"),
            ("[1,,2]", "Unexpected token ','"),
            ("[1,]", "Unexpected comma at the end of array expression"),
            ("[1", "Expected ']'"),
            ("[1 2]", "Expected ']'"),
            ("[01]", "Expected ']'"),
            ("[1e]", "Expected ']'"),
            ("[truey]", "Expected ']'"),
            ("[1 notjson]", "Expected ']'"),
            ("[1;]", "Expected ']'"),
            ("[1 (]", "Expected ']'"),
            ("[1 @]", "Unrecognized token '@'"),
            ("[1-]", "Invalid number"),
            ("[}", "Unexpected token '}'"),
            ("{", "Expected '}'"),
            ("{ ", "Expected '}'"),
            ("{a:1}", "Expected '}'"),
            ("{,}", "Expected '}'"),
            ("{;}", "Expected '}'"),
            (
                "{\"a\"",
                "Expected ':' before value in object property definition",
            ),
            (
                "{\"a\" 1}",
                "Expected ':' before value in object property definition",
            ),
            (
                "{\"a\" \"b\"}",
                "Expected ':' before value in object property definition",
            ),
            ("{\"a\":", "Unexpected EOF"),
            ("{\"a\":}", "Unexpected token '}'"),
            ("{\"a\":1", "Expected '}'"),
            ("{\"a\":1 ", "Expected '}'"),
            ("{\"a\":1 \"b\":2}", "Expected '}'"),
            ("{\"a\":1,}", "Property name must be a string literal"),
            ("{\"a\":1,]", "Property name must be a string literal"),
            ("{\"a\":1,", "Property name must be a string literal"),
            ("{\"a\":1,-}", "Invalid number"),
            ("{\"a\":1 x}", "Expected '}'"),
            ("{\"a\":nope}", "Unexpected identifier \"nope\""),
            ("1 2", "Unable to parse JSON string"),
            ("\"a\" \"b\"", "Unable to parse JSON string"),
            ("{} {}", "Unable to parse JSON string"),
            ("1 ;", "Unable to parse JSON string"),
            ("1}", "Unable to parse JSON string"),
            ("\"a\" @", "Unrecognized token '@'"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                jsc_parse_error(input).as_deref(),
                Some(*expected),
                "input {input:?}"
            );
        }
    }

    /// Valid inputs carry no JSC error (the lone-surrogate corner
    /// stays serde's to reject).
    #[test]
    fn jsc_parse_error_accepts_valid_json() {
        for input in [
            "0",
            "-0",
            "1e999",
            "[1,-2]",
            "[1.2e3]",
            "  {\"a\":[1,{\"b\":\"c\"}]}  ",
            "\"\\ud800\"",
            "{\"\":1}",
        ] {
            assert_eq!(jsc_parse_error(input), None, "input {input:?}");
        }
    }
}
