//! Find the end of one ASCII numeric token without materializing the input.
//! Literal parsing retains responsibility for radix, separators, and range checks.

pub(super) fn end(bytes: &[u8], mut offset: usize) -> usize {
    let first = bytes[offset];
    offset += 1;
    if first == b'0' && matches!(bytes.get(offset), Some(b'x' | b'X' | b'b' | b'B')) {
        offset += 1;
        while bytes
            .get(offset)
            .is_some_and(|byte| byte.is_ascii_hexdigit() || *byte == b'_')
        {
            offset += 1;
        }
    } else {
        digits(bytes, &mut offset);
        if bytes.get(offset) == Some(&b'.') && bytes.get(offset + 1).is_some_and(u8::is_ascii_digit) {
            offset += 1;
            digits(bytes, &mut offset);
        }
        if matches!(bytes.get(offset), Some(b'e' | b'E')) {
            offset += 1;
            if matches!(bytes.get(offset), Some(b'+' | b'-')) {
                offset += 1;
            }
            digits(bytes, &mut offset);
        }
    }
    offset
}
fn digits(bytes: &[u8], offset: &mut usize) {
    while bytes
        .get(*offset)
        .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'_')
    {
        *offset += 1;
    }
}
