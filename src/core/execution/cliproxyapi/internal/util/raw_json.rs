// Origin: CTOX
// License: AGPL-3.0-only
// JSON grammar follows gjson 0.8.1 src/valid.rs; stack traversal is iterative.

#[derive(Clone, Copy)]
enum State {
    RootValue,
    RootDone,
    ArrayFirst,
    ArrayValue,
    ArrayAfter,
    ObjectFirst,
    ObjectKey,
    ObjectColon,
    ObjectValue,
    ObjectAfter,
}

/// Validates raw JSON without decoding numbers or recursing on the thread stack.
/// Like Go gjson.ValidBytes, this checks JSON byte grammar rather than UTF-8 or
/// Unicode scalar validity inside strings. Duplicate keys and huge numbers stay raw.
pub fn valid_json_bytes(json: &[u8]) -> bool {
    let mut states = vec![State::RootValue];
    let mut offset = 0;
    while let Some(state) = states.last().copied() {
        while json
            .get(offset)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        {
            offset += 1;
        }
        match state {
            State::RootDone => return states.len() == 1 && offset == json.len(),
            State::ArrayFirst if json.get(offset) == Some(&b']') => {
                offset += 1;
                states.pop();
            }
            State::ObjectFirst if json.get(offset) == Some(&b'}') => {
                offset += 1;
                states.pop();
            }
            State::ObjectFirst | State::ObjectKey => {
                if !consume_string(json, &mut offset) {
                    return false;
                }
                *states.last_mut().unwrap() = State::ObjectColon;
            }
            State::ObjectColon => {
                if json.get(offset) != Some(&b':') {
                    return false;
                }
                offset += 1;
                *states.last_mut().unwrap() = State::ObjectValue;
            }
            State::ArrayAfter | State::ObjectAfter => {
                let (end, next) = match state {
                    State::ArrayAfter => (b']', State::ArrayValue),
                    _ => (b'}', State::ObjectKey),
                };
                if json.get(offset) == Some(&end) {
                    offset += 1;
                    states.pop();
                } else if json.get(offset) == Some(&b',') {
                    offset += 1;
                    *states.last_mut().unwrap() = next;
                } else {
                    return false;
                }
            }
            State::RootValue | State::ArrayFirst | State::ArrayValue | State::ObjectValue => {
                let Some(byte) = json.get(offset).copied() else {
                    return false;
                };
                *states.last_mut().unwrap() = match state {
                    State::RootValue => State::RootDone,
                    State::ObjectValue => State::ObjectAfter,
                    _ => State::ArrayAfter,
                };
                match byte {
                    b'[' => {
                        offset += 1;
                        states.push(State::ArrayFirst);
                    }
                    b'{' => {
                        offset += 1;
                        states.push(State::ObjectFirst);
                    }
                    b'"' => {
                        if !consume_string(json, &mut offset) {
                            return false;
                        }
                    }
                    b'-' | b'0'..=b'9' => {
                        if !consume_number(json, &mut offset) {
                            return false;
                        }
                    }
                    b't' | b'f' | b'n' => {
                        let literal: &[u8] = match byte {
                            b't' => b"true",
                            b'f' => b"false",
                            _ => b"null",
                        };
                        if !json[offset..].starts_with(literal) {
                            return false;
                        }
                        offset += literal.len();
                    }
                    _ => return false,
                }
            }
        }
    }
    false
}

fn consume_string(json: &[u8], offset: &mut usize) -> bool {
    if json.get(*offset) != Some(&b'"') {
        return false;
    }
    *offset += 1;
    while let Some(byte) = json.get(*offset).copied() {
        *offset += 1;
        match byte {
            b'"' => return true,
            0..=0x1f => return false,
            b'\\' => {
                let Some(escape) = json.get(*offset).copied() else {
                    return false;
                };
                *offset += 1;
                match escape {
                    b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {}
                    b'u' => {
                        let Some(hex) = json.get(*offset..).and_then(|rest| rest.get(..4)) else {
                            return false;
                        };
                        if !hex.iter().all(u8::is_ascii_hexdigit) {
                            return false;
                        }
                        *offset += 4;
                    }
                    _ => return false,
                }
            }
            _ => {}
        }
    }
    false
}
fn consume_number(json: &[u8], offset: &mut usize) -> bool {
    if json.get(*offset) == Some(&b'-') {
        *offset += 1;
    }
    match json.get(*offset) {
        Some(b'0') => *offset += 1,
        Some(b'1'..=b'9') => {
            while json.get(*offset).is_some_and(u8::is_ascii_digit) {
                *offset += 1;
            }
        }
        _ => return false,
    }
    if json.get(*offset) == Some(&b'.') {
        *offset += 1;
        if !json.get(*offset).is_some_and(u8::is_ascii_digit) {
            return false;
        }
        while json.get(*offset).is_some_and(u8::is_ascii_digit) {
            *offset += 1;
        }
    }
    if json
        .get(*offset)
        .is_some_and(|byte| matches!(byte, b'e' | b'E'))
    {
        *offset += 1;
        if json
            .get(*offset)
            .is_some_and(|byte| matches!(byte, b'+' | b'-'))
        {
            *offset += 1;
        }
        if !json.get(*offset).is_some_and(u8::is_ascii_digit) {
            return false;
        }
        while json.get(*offset).is_some_and(u8::is_ascii_digit) {
            *offset += 1;
        }
    }
    true
}
