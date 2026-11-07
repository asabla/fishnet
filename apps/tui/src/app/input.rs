use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(super) fn paste<const N: usize>(
    fields: &mut [String; N],
    focus: usize,
    cursor: &mut usize,
    text: &str,
) {
    if let Some(field) = fields.get_mut(focus) {
        insert(field, cursor, text, 512, false);
    }
}

pub(super) fn insert(
    field: &mut String,
    cursor: &mut usize,
    text: &str,
    limit: usize,
    multiline: bool,
) {
    for ch in text
        .chars()
        .filter(|ch| !ch.is_control() || (multiline && *ch == '\n'))
    {
        let ch = clean_char(ch);
        if field.len() + ch.len_utf8() > limit {
            break;
        }
        field.insert(*cursor, ch);
        *cursor += ch.len_utf8();
    }
}
pub(super) fn edit<const N: usize>(
    fields: &mut [String; N],
    focus: &mut usize,
    cursor: &mut usize,
    key: KeyEvent,
) {
    if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
        *focus = (*focus + if key.code == KeyCode::Tab { 1 } else { N - 1 }) % N;
        *cursor = fields.get(*focus).map_or(0, String::len);
        return;
    }
    let Some(field) = fields.get_mut(*focus) else {
        return;
    };
    edit_text(field, cursor, key, 512);
}

pub(super) fn edit_text(field: &mut String, cursor: &mut usize, key: KeyEvent, limit: usize) {
    let previous = field
        .get(..*cursor)
        .and_then(|text| text.char_indices().last())
        .map_or(0, |(index, _)| index);
    let next = field
        .get(*cursor..)
        .and_then(|text| text.chars().next())
        .map_or(*cursor, |ch| *cursor + ch.len_utf8());
    match key.code {
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = field.len(),
        KeyCode::Left => *cursor = previous,
        KeyCode::Right => *cursor = next,
        KeyCode::Backspace if *cursor > 0 => {
            field.replace_range(previous..*cursor, "");
            *cursor = previous;
        }
        KeyCode::Delete if *cursor < field.len() => {
            field.replace_range(*cursor..next, "");
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            field.clear();
            *cursor = 0;
        }
        KeyCode::Char(ch)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                && !ch.is_control()
                && field.len() + ch.len_utf8() <= limit =>
        {
            let ch = clean_char(ch);
            field.insert(*cursor, ch);
            *cursor += ch.len_utf8();
        }
        _ => (),
    }
}

fn clean_char(ch: char) -> char {
    if ch == '\n' {
        ch
    } else {
        super::safe(&ch.to_string(), 4)
            .chars()
            .next()
            .unwrap_or(' ')
    }
}
