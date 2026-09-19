use std::collections::HashMap;

pub const TELOPT_MSDP: u8 = 69;
pub const MSDP_VAR: u8 = 1;
pub const MSDP_VAL: u8 = 2;
pub const MSDP_TABLE_OPEN: u8 = 3;
pub const MSDP_TABLE_CLOSE: u8 = 4;
pub const MSDP_ARRAY_OPEN: u8 = 5;
pub const MSDP_ARRAY_CLOSE: u8 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MsdpValue {
    String(String),
    Array(Vec<MsdpValue>),
    Table(HashMap<String, MsdpValue>),
}

impl MsdpValue {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            Self::Array(_) | Self::Table(_) => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        self.as_string()?.trim().parse().ok()
    }

    pub fn as_table(&self) -> Option<&HashMap<String, MsdpValue>> {
        match self {
            Self::Table(value) => Some(value),
            Self::String(_) | Self::Array(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsdpFrame {
    pub variable: String,
    pub value: MsdpValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsdpParseError {
    pub message: String,
}

impl MsdpParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub fn encode_pair(variable: &str, value: &str) -> Vec<u8> {
    let mut payload = Vec::with_capacity(variable.len() + value.len() + 2);
    payload.push(MSDP_VAR);
    payload.extend_from_slice(variable.as_bytes());
    payload.push(MSDP_VAL);
    payload.extend_from_slice(value.as_bytes());
    payload
}

pub fn parse_msdp_payload(payload: &[u8]) -> Result<Vec<MsdpFrame>, MsdpParseError> {
    let mut cursor = Cursor::new(payload);
    let mut frames = Vec::new();

    while !cursor.is_done() {
        if cursor.peek() != Some(MSDP_VAR) {
            cursor.advance();
            continue;
        }
        cursor.advance();
        let variable = cursor.read_string_until(&[MSDP_VAL, MSDP_VAR]);
        if variable.is_empty() || cursor.peek() != Some(MSDP_VAL) {
            continue;
        }
        cursor.advance();
        let value = cursor.read_value_until_top_level_var()?;
        frames.push(MsdpFrame { variable, value });
    }

    Ok(frames)
}

struct Cursor<'a> {
    payload: &'a [u8],
    index: usize,
}

impl<'a> Cursor<'a> {
    fn new(payload: &'a [u8]) -> Self {
        Self { payload, index: 0 }
    }

    fn is_done(&self) -> bool {
        self.index >= self.payload.len()
    }

    fn peek(&self) -> Option<u8> {
        self.payload.get(self.index).copied()
    }

    fn advance(&mut self) {
        self.index = self.index.saturating_add(1);
    }

    fn read_string_until(&mut self, stop: &[u8]) -> String {
        let start = self.index;
        while let Some(byte) = self.peek() {
            if stop.contains(&byte) {
                break;
            }
            self.advance();
        }
        String::from_utf8_lossy(&self.payload[start..self.index]).into_owned()
    }

    fn read_value_until_top_level_var(&mut self) -> Result<MsdpValue, MsdpParseError> {
        match self.peek() {
            Some(MSDP_ARRAY_OPEN) => self.read_array(),
            Some(MSDP_TABLE_OPEN) => self.read_table(),
            Some(_) | None => Ok(MsdpValue::String(self.read_string_until(&[
                MSDP_VAR,
                MSDP_TABLE_CLOSE,
                MSDP_ARRAY_CLOSE,
            ]))),
        }
    }

    fn read_nested_value(&mut self) -> Result<MsdpValue, MsdpParseError> {
        match self.peek() {
            Some(MSDP_ARRAY_OPEN) => self.read_array(),
            Some(MSDP_TABLE_OPEN) => self.read_table(),
            Some(_) | None => Ok(MsdpValue::String(self.read_string_until(&[
                MSDP_VAL,
                MSDP_VAR,
                MSDP_TABLE_CLOSE,
                MSDP_ARRAY_CLOSE,
            ]))),
        }
    }

    fn read_array(&mut self) -> Result<MsdpValue, MsdpParseError> {
        self.expect(MSDP_ARRAY_OPEN)?;
        let mut values = Vec::new();

        while !self.is_done() {
            match self.peek() {
                Some(MSDP_ARRAY_CLOSE) => {
                    self.advance();
                    return Ok(MsdpValue::Array(values));
                }
                Some(MSDP_VAL) => {
                    self.advance();
                    values.push(self.read_nested_value()?);
                }
                Some(MSDP_TABLE_OPEN) | Some(MSDP_ARRAY_OPEN) => {
                    values.push(self.read_nested_value()?);
                }
                Some(_) => {
                    values.push(self.read_nested_value()?);
                }
                None => break,
            }
        }

        Err(MsdpParseError::new("unterminated MSDP array"))
    }

    fn read_table(&mut self) -> Result<MsdpValue, MsdpParseError> {
        self.expect(MSDP_TABLE_OPEN)?;
        let mut values = HashMap::new();

        while !self.is_done() {
            match self.peek() {
                Some(MSDP_TABLE_CLOSE) => {
                    self.advance();
                    return Ok(MsdpValue::Table(values));
                }
                Some(MSDP_VAR) => {
                    self.advance();
                    let key = self.read_string_until(&[MSDP_VAL, MSDP_VAR, MSDP_TABLE_CLOSE]);
                    if key.is_empty() || self.peek() != Some(MSDP_VAL) {
                        continue;
                    }
                    self.advance();
                    values.insert(key, self.read_nested_value()?);
                }
                Some(_) => self.advance(),
                None => break,
            }
        }

        Err(MsdpParseError::new("unterminated MSDP table"))
    }

    fn expect(&mut self, expected: u8) -> Result<(), MsdpParseError> {
        if self.peek() == Some(expected) {
            self.advance();
            Ok(())
        } else {
            Err(MsdpParseError::new("unexpected MSDP marker"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_string_variables() {
        let mut payload = encode_pair("HEALTH", "75");
        payload.extend_from_slice(&encode_pair("MANA", "40"));

        let frames = parse_msdp_payload(&payload).expect("payload should parse");

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].variable, "HEALTH");
        assert_eq!(frames[0].value, MsdpValue::String("75".to_string()));
        assert_eq!(frames[1].variable, "MANA");
    }

    #[test]
    fn parses_arrays_tables_and_nested_values() {
        let mut payload = Vec::new();
        payload.push(MSDP_VAR);
        payload.extend_from_slice(b"GROUP");
        payload.push(MSDP_VAL);
        payload.push(MSDP_TABLE_OPEN);
        payload.push(MSDP_VAR);
        payload.extend_from_slice(b"MEMBERS");
        payload.push(MSDP_VAL);
        payload.push(MSDP_ARRAY_OPEN);
        payload.push(MSDP_VAL);
        payload.push(MSDP_TABLE_OPEN);
        payload.extend_from_slice(&encode_pair("NAME", "Boromir"));
        payload.extend_from_slice(&encode_pair("HEALTH", "50"));
        payload.push(MSDP_TABLE_CLOSE);
        payload.push(MSDP_ARRAY_CLOSE);
        payload.push(MSDP_TABLE_CLOSE);

        let frames = parse_msdp_payload(&payload).expect("payload should parse");
        let MsdpValue::Table(group) = &frames[0].value else {
            panic!("GROUP should be a table");
        };
        let Some(MsdpValue::Array(members)) = group.get("MEMBERS") else {
            panic!("MEMBERS should be an array");
        };
        assert_eq!(members.len(), 1);
    }

    #[test]
    fn reports_malformed_nested_values_without_panic() {
        let payload = [MSDP_VAR, b'X', MSDP_VAL, MSDP_ARRAY_OPEN, MSDP_VAL, b'1'];
        let error = parse_msdp_payload(&payload).expect_err("array should be unterminated");
        assert!(error.message.contains("unterminated"));
    }
}
