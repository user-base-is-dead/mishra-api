//! streaming response processing module
//!
//! implement Kiro → Anthropic streaming response conversion and SSE statemanage

use std::collections::HashMap;

use serde_json::json;
use uuid::Uuid;

use crate::kiro::model::events::Event;

/// thinking block signature occupybitstring
///
/// Anthropic Messages API protocolspecify thinking under the mode,assistant message
/// `{type:"thinking", ...}` blockmust carry `signature` field and returns it as is on the next round,
/// otherwise SDK / The server rejects the request and reports:
/// `The content[].thinking in the thinking mode must be passed back to the API`.
///
/// upstream Kiro Does not deliver a real signature (it itself is not Anthropic server side), therefore kiro.rs in
/// thinking Inserts a non empty placeholder string at block end to satisfy client local validation.
/// converter during parse assistant messagebackpass Kiro read only when `block.thinking`, do not read
/// signature, so this placeholder string is only on the client. ↔ kiro.rs exists between, does not affect forwarding.
pub(super) const THINKING_SIGNATURE_PLACEHOLDER: &str = "kiro-rs-thinking-signature";

/// Finds the nearest valid one less than or equal to the target position.UTF-8characterboundary
///
/// UTF-8charactercancanoccupyuse1-4bytes; slicing directly by byte position may cut in the middle of a multi byte character and causepanic.
/// This function searches backward from the target position to find the nearest valid character boundary.
fn find_char_boundary(s: &str, target: usize) -> usize {
    if target >= s.len() {
        return s.len();
    }
    if target == 0 {
        return 0;
    }
    // Searches backward from the target position for a valid character boundary.
    let mut pos = target;
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

/// wrapping characters that need to be skipped
///
/// when thinking When a tag is wrapped by these characters, it is considered a quoted tag rather than a real tag:
/// - backtick (`):lineinsidecode
/// - double quote ("):string
/// - single quote ('):string
const QUOTE_CHARS: &[u8] = &[
    b'`', b'"', b'\'', b'\\', b'#', b'!', b'@', b'$', b'%', b'^', b'&', b'*', b'(', b')', b'-',
    b'_', b'=', b'+', b'[', b']', b'{', b'}', b';', b':', b'<', b'>', b',', b'.', b'?', b'/',
];

/// Checks whether the character at the given position is a quote character.
fn is_quote_char(buffer: &str, pos: usize) -> bool {
    buffer
        .as_bytes()
        .get(pos)
        .map(|c| QUOTE_CHARS.contains(c))
        .unwrap_or(false)
}

/// findreal thinking End tag (not wrapped by quote characters and followed by a double newline).
///
/// When the model mentions during thinking `</thinking>` usually wrapped with backticks, quotes, and so on,
/// or there is other content on the same line (such as"about </thinking> tag").
/// This function skips these cases and returns only the position of the real end tag.
///
/// skipofsituation:
/// - Wrapped by quote characters (backticks, quotes, and so on).
/// - There is no double newline after it (a real end tag would be followed by one). `\n\n`)
/// - The tag is at the end of the buffer (streaming needs to wait for more content).
///
/// # parameter
/// - `buffer`: the string to search for
///
/// # return value
/// - `Some(pos)`: The start position of the real end tag.
/// - `None`: No real end tag was found.
fn find_real_thinking_end_tag(buffer: &str) -> Option<usize> {
    const TAG: &str = "</thinking>";
    let mut search_start = 0;

    while let Some(pos) = buffer[search_start..].find(TAG) {
        let absolute_pos = search_start + pos;

        // Checks whether there is a quote character before.
        let has_quote_before = absolute_pos > 0 && is_quote_char(buffer, absolute_pos - 1);

        // Checks whether there is a quote character after.
        let after_pos = absolute_pos + TAG.len();
        let has_quote_after = is_quote_char(buffer, after_pos);

        // If wrapped by quote characters, skips.
        if has_quote_before || has_quote_after {
            search_start = absolute_pos + 1;
            continue;
        }

        // check the following content
        let after_content = &buffer[after_pos..];

        // If the content after the tag is not enough to decide whether there is a double newline, wait for more content.
        if after_content.len() < 2 {
            return None;
        }

        // real thinking A real end tag is followed by a double newline. `\n\n`
        if after_content.starts_with("\n\n") {
            return Some(absolute_pos);
        }

        // Not a double newline; skips and keeps searching.
        search_start = absolute_pos + 1;
    }

    None
}

/// find at the end of the buffer thinking End tag (allows only whitespace at the end).
///
/// used for“boundaryevent”scenario:for example thinking immediately enter after ending tool_use,orstreamend,
/// at this point `</thinking>` afterside cancannone `\n\n`, but the end tag should still be recognized and filtered.
///
/// constraint:onlywhen `</thinking>` Only when everything after is whitespace is it considered an end tag,
/// toavoid in thinking contentinmention `</thinking>`(not an end tag) case misjudgment.
fn find_real_thinking_end_tag_at_buffer_end(buffer: &str) -> Option<usize> {
    const TAG: &str = "</thinking>";
    let mut search_start = 0;

    while let Some(pos) = buffer[search_start..].find(TAG) {
        let absolute_pos = search_start + pos;

        // Checks whether there is a quote character before.
        let has_quote_before = absolute_pos > 0 && is_quote_char(buffer, absolute_pos - 1);

        // Checks whether there is a quote character after.
        let after_pos = absolute_pos + TAG.len();
        let has_quote_after = is_quote_char(buffer, after_pos);

        if has_quote_before || has_quote_after {
            search_start = absolute_pos + 1;
            continue;
        }

        // Only when everything after the tag is whitespace is it recognized as an end tag.
        if buffer[after_pos..].trim().is_empty() {
            return Some(absolute_pos);
        }

        search_start = absolute_pos + 1;
    }

    None
}

/// findreal thinking Start tag (not wrapped by quote characters).
///
/// and `find_real_thinking_end_tag` Similar, skips a start tag wrapped by quote characters.
fn find_real_thinking_start_tag(buffer: &str) -> Option<usize> {
    const TAG: &str = "<thinking>";
    let mut search_start = 0;

    while let Some(pos) = buffer[search_start..].find(TAG) {
        let absolute_pos = search_start + pos;

        // Checks whether there is a quote character before.
        let has_quote_before = absolute_pos > 0 && is_quote_char(buffer, absolute_pos - 1);

        // Checks whether there is a quote character after.
        let after_pos = absolute_pos + TAG.len();
        let has_quote_after = is_quote_char(buffer, after_pos);

        // If not wrapped by quote characters, it is a real start tag.
        if !has_quote_before && !has_quote_after {
            return Some(absolute_pos);
        }

        // continue searching for the next match
        search_start = absolute_pos + 1;
    }

    None
}

/// check `name_pos`whether what precedes it (pointing at the first letter of the tag name) forms a valid open tag start,
/// compat bareway of writing `<tag` and the form with a namespace prefix. `<prefix:tag`.
///
/// return `Some(lt_pos)`(points to `<` byte position) indicates valid;`None` means it is not a tag.
fn open_tag_lt_pos(buffer: &str, name_pos: usize) -> Option<usize> {
    let bytes = buffer.as_bytes();
    if name_pos == 0 {
        return None;
    }
    let prev = bytes[name_pos - 1];
    if prev == b'<' {
        return Some(name_pos - 1);
    }
    // like `<prefix:tag`:name preceding is ':', further back is an identifier, further back is '<'
    if prev == b':' {
        let i = name_pos - 1; // point to ':'
        let mut j = i; // identifier left boundary scan
        while j > 0 && {
            let c = bytes[j - 1];
            c.is_ascii_alphanumeric() || c == b'_'
        } {
            j -= 1;
        }
        // The identifier is non empty and to its left is '<'
        if j < i && j > 0 && bytes[j - 1] == b'<' {
            return Some(j - 1);
        }
    }
    None
}

/// Finds the one not wrapped by quote characters. invoke opening tag, return pointing to `<` ofbytesbitset
///
/// compat bare `<invoke ...>` with a namespace prefix `<prefix:invoke ...>` two ways of writing.
/// reuse `is_quote_char`: if `<` beforeclosely attachedbacktick/Wrapping characters such as quotes are treated as quoting and skipped.
fn find_invoke_start(buffer: &str) -> Option<usize> {
    let mut search = 0;
    while let Some(rel) = buffer[search..].find("invoke") {
        let name_pos = search + rel;
        if let Some(lt) = open_tag_lt_pos(buffer, name_pos) {
            // After the tag name must be a boundary character (whitespace or '>'), to avoid a false match invoked such as
            let after = name_pos + "invoke".len();
            let next_ok = buffer.as_bytes().get(after).map_or(true, |c| {
                c.is_ascii_whitespace() || *c == b'>' || *c == b'/'
            });
            let has_quote_before = lt > 0 && is_quote_char(buffer, lt - 1);
            if next_ok && !has_quote_before {
                return Some(lt);
            }
        }
        search = name_pos + "invoke".len();
    }
    None
}

/// from `start` after that find the first invoke close tag, returns the end position (exclusive,containsclosetag)
///
/// compat bare `</invoke>` andcarryprefix `</prefix:invoke>`.findnottoreturn `None`(the block has not fully arrived).
fn find_invoke_block_end(buffer: &str, start: usize) -> Option<usize> {
    // block A boundary = next `<invoke` opening tag (that is the next block B the start point), if none then to buffer end.
    // so that consecutive sends burst(A immediately follow B) when,A ofsearch intervalby B blocked by the open tag, never eats into B.
    let boundary = match find_next_invoke_open(buffer, start) {
        Some(p) => p,
        None => buffer.len(),
    };
    // in [start, boundary) take the last one in the interval `</invoke>` astrueclosed.
    // greedily take the last one → patch the literal appearing in the body `</invoke>` will not cause early truncation;
    // The interval is blocked by the next block open tag. → will not be wrongly merged across blocks.
    find_last_invoke_close(buffer, start, boundary)
}

/// from `start` after that find the next real `<invoke`(or `<prefix:invoke`) the byte position of the opening tag.
/// skip `start` the current block own open tag.
fn find_next_invoke_open(buffer: &str, start: usize) -> Option<usize> {
    // First skips the current block open tag: from start afternumbera '>' afterstart searching.
    let after_open = match buffer[start..].find('>') {
        Some(rel) => start + rel + 1,
        None => return None,
    };
    // note: cannot reuse find_invoke_start——it for `<` preceding is `>`the quote character case is rejected,
    // but consecutive send burst in B of `<invoke` exactlyimmediately followin A of `</invoke>` of `>` after.
    // here we only recognize the structure:`<invoke` or `<prefix:invoke`, after the opening tag name there must be whitespace/`>`/`/` boundary.
    let region = &buffer[after_open..];
    let mut search = 0usize;
    while let Some(rel) = region[search..].find("invoke") {
        let name_pos = search + rel;
        if let Some(lt) = open_tag_lt_pos(region, name_pos) {
            let after = name_pos + "invoke".len();
            let next_ok = region.as_bytes().get(after).map_or(true, |c| {
                c.is_ascii_whitespace() || *c == b'>' || *c == b'/'
            });
            if next_ok {
                return Some(after_open + lt);
            }
        }
        search = name_pos + "invoke".len();
    }
    None
}

/// in `[from, boundary)` find the last one within the interval `</invoke>` / `</prefix:invoke>` ofendbitset
/// (exclusive, including the close tag). Returns when not found `None`(the block has not fully arrived).
fn find_last_invoke_close(buffer: &str, from: usize, boundary: usize) -> Option<usize> {
    let region_end = boundary.min(buffer.len());
    if from >= region_end {
        return None;
    }
    let region = &buffer[from..region_end];
    let bytes = region.as_bytes();
    let mut search = 0usize;
    let mut last: Option<usize> = None;
    while let Some(rel) = region[search..].find("invoke>") {
        let name_pos = search + rel;
        // '</invoke>' form
        if name_pos >= 2 && &region[name_pos - 2..name_pos] == "</" {
            last = Some(from + name_pos + "invoke>".len());
        } else if name_pos >= 1 && bytes[name_pos - 1] == b':' {
            // '</prefix:invoke>' form
            let mut j = name_pos - 1; // ':'
            while j > 0 && {
                let c = bytes[j - 1];
                c.is_ascii_alphanumeric() || c == b'_'
            } {
                j -= 1;
            }
            if j >= 2 && &region[j - 2..j] == "</" {
                last = Some(from + name_pos + "invoke>".len());
            }
        }
        search = name_pos + "invoke>".len();
    }
    last
}

/// extract from the tag string `name="..."` the value (take the first match)
fn extract_name_attr(tag: &str) -> Option<String> {
    let needle = "name=\"";
    let rel = tag.find(needle)?;
    let start = rel + needle.len();
    let end_rel = tag[start..].find('"')?;
    Some(tag[start..start + end_rel].to_string())
}

/// parseacomplete invoke block,extractout (tool_name, input_json_string)
///
/// - tool name from invoke opening tagof `name="..."`(compat antml: prefix)
/// - the parameters are zero or more `<parameter name="K">V</parameter>`(compatprefix)
/// - The parameter value extends up to before the next parameter open tag.**mostaftera** `</parameter>` as the boundary (greedy),
///   allowmulti line / contains `<` / Chinese / contains literal `</parameter>`(P0-1 fix)
/// - use serde_json assemble into object(the values are all strings, auto escaped)
/// - no valid name orconcatenatenotoutvalid JSON return `None`
fn parse_invoke_block(block: &str) -> Option<(String, String)> {
    // invoke opening tag = from the block start to the first '>'
    let open_end = block.find('>')?;
    let open_tag = &block[..=open_end];
    let tool_name = extract_name_attr(open_tag)?;
    if tool_name.is_empty() {
        return None;
    }

    let mut map = serde_json::Map::new();
    let body = &block[open_end + 1..];
    let mut cursor = 0usize;
    while let Some(rel) = body[cursor..].find("parameter name=\"") {
        let name_kw = cursor + rel;
        // confirmisreal '<parameter' or '<prefix:parameter' opening tag
        // name_kw point to 'parameter',towardbeforeshouldis '<' or '<prefix:'
        // confirm it is a real opening tag ('<parameter' / '<prefix:parameter'); used only for validation, no position value needed.
        if open_tag_lt_pos(body, name_kw).is_none() {
            cursor = name_kw + "parameter".len();
            continue;
        }
        // find the opening tag of this parameter '>'
        let tag_gt = match body[name_kw..].find('>') {
            Some(r) => name_kw + r,
            None => break, // the opening tag is not closed, stop
        };
        let param_open_tag = &body[name_kw..tag_gt + 1];
        // from 'parameter name="..."' extract key(strips prefix interference: directly finds name=")
        let key = match extract_name_attr(param_open_tag) {
            Some(k) => k,
            None => {
                cursor = tag_gt + 1;
                continue;
            }
        };
        // parametervalue taketo </parameter>(compatible prefix) as the boundary.find_param_close relatively expensive, called only once,
        // samewhenreuse (closetagstart, closetagend) Two values: the start is used to slice the value, the end is used to advance the cursor.
        let val_start = tag_gt + 1;
        let (close_start, close_end) = match find_param_close(body, val_start) {
            Some(pair) => pair,
            None => break, // the value is not closed, stop
        };
        let value = &body[val_start..close_start];
        map.insert(key, serde_json::Value::String(value.to_string()));
        // advance to after the closing tag
        cursor = close_end;
    }

    let obj = serde_json::Value::Object(map);
    let s = serde_json::to_string(&obj).ok()?;
    Some((tool_name, s))
}

/// from `from` start finding the first parameter closetag, return (startbitset, endbitset exclusive)
///
/// compat bare `</parameter>` andcarryprefix `</prefix:parameter>`.
fn find_param_close(body: &str, from: usize) -> Option<(usize, usize)> {
    // P0-1: the parameter value (especially apply_patch of patch body) may contain a literal `</parameter>`.
    // the naive take the first </parameter>would truncate the value. Changed to greedily take the last one within the boundary. </parameter>:
    // boundary = next `<parameter name="` open tag (multi parameter case); if none, up to body end.
    // like this:① singleparameter(including apply_patch) captures the truly last closing; literal closings inside the content are not harmed;
    //      ② Multiple parameters are still split correctly by the next parameter open tag.
    // Limitation (honestly noted): if the parameter value also contains a literal `<parameter name="`, the boundary determination will be too early;
    // measured apply_patch The body rarely contains this literal string, so it is acceptable.
    let boundary = match find_next_param_open(body, from) {
        Some(p) => p,
        None => body.len(),
    };
    let region = &body[from..boundary];
    let kw = "parameter>";
    let mut last: Option<(usize, usize)> = None;
    let mut search = 0usize;
    let bytes = region.as_bytes();
    while let Some(rel) = region[search..].find(kw) {
        let name_pos = search + rel;
        // '</parameter>' form
        if name_pos >= 2 && &region[name_pos - 2..name_pos] == "</" {
            last = Some((from + name_pos - 2, from + name_pos + kw.len()));
        } else if name_pos >= 1 && bytes[name_pos - 1] == b':' {
            // '</prefix:parameter>' form
            let mut j = name_pos - 1; // ':'
            while j > 0 && {
                let c = bytes[j - 1];
                c.is_ascii_alphanumeric() || c == b'_'
            } {
                j -= 1;
            }
            if j >= 2 && &region[j - 2..j] == "</" {
                last = Some((from + j - 2, from + name_pos + kw.len()));
            }
        }
        search = name_pos + kw.len();
    }
    last
}

/// from `from` start finding the next `<parameter name="`(or `<prefix:parameter name="`) the byte position of the opening tag.
/// used for `find_param_close` greedy boundary: the current parameter value at most extends up to before the next parameter open tag.
fn find_next_param_open(body: &str, from: usize) -> Option<usize> {
    let mut search = from;
    while let Some(rel) = body[search..].find("parameter name=\"") {
        let kw_pos = search + rel;
        // must be a real opening tag:'parameter' preceding is '<' or '<prefix:'
        if let Some(lt) = open_tag_lt_pos(body, kw_pos) {
            return Some(lt);
        }
        search = kw_pos + "parameter".len();
    }
    None
}

/// Strips the standalone one at the tail of the text before the block. stray token line (a line of its own `call` or `count`)
///
/// in measurement `<invoke>` a bare line often appears before `call`/`count`, needs to be stripped from the narrative text before the block,
/// Avoids leaking to the client. Only strips“at the tail, and on a line of its own”of stray token, the preceding normal narrative is kept.
/// alreadymeasuredtoof stray token set:Opus When the long context degrades, the leaked `<invoke>` a bare line often exists before
/// `call` / `count` / `card`. The set form makes future extension easy.
const STRAY_INVOKE_TOKENS: &[&str] = &["call", "count", "card"];

/// repeat readout circuit breaker threshold: the same stray token(call/count/card) repeats consecutively as a standalone line.
/// exceeding this many times is judged asOpus long context degraded repeat loop, immediately circuit breaks this round text output.
///
/// Value tradeoff: before a normal tool call at most appears 1 guide word lines (occasionally 2~3), never dozens of times in a row.
/// set to 32 Far above the normal ceiling and far below the tens of thousands seen when degraded, neither harming normal guide words nor delaying the stop.
const REPEAT_GUARD_TRIP_THRESHOLD: u32 = 32;

/// Block level repeat folding: performs a one shot repeat circuit breaking on the complete whole text.
///
/// used fornon streaming / web_search loop path (`extract_invoke_content_blocks` entry)——
/// that path does not go through streaming `emit_text_delta_raw` per chunk circuit breaking, so it is caught independently here once.
///
/// The rule is consistent with the streaming version: the same `STRAY_INVOKE_TOKENS`(call/count/card) consecutively as a line of its own
/// duplicateexceeds `REPEAT_GUARD_TRIP_THRESHOLD` times,decideas Opus degrade to repetition,**truncate from where the threshold is exceeded**,
/// Discards all repeated garbage after it (breaks the snowball, does not fill history). A small number of guide word repeats within the threshold are kept as is.
fn collapse_stray_token_floods(text: &str) -> std::borrow::Cow<'_, str> {
    let mut last_line = "";
    let mut run: u32 = 0;
    let mut cut_at: Option<usize> = None;
    let mut offset = 0usize;
    for segment in text.split_inclusive('\n') {
        let line = segment.trim();
        if STRAY_INVOKE_TOKENS.contains(&line) {
            if line == last_line {
                run += 1;
            } else {
                last_line = line;
                run = 1;
            }
            if run >= REPEAT_GUARD_TRIP_THRESHOLD {
                // Truncates from the start of this segment (this line): keeps the content accumulated within the threshold.
                cut_at = Some(offset);
                break;
            }
        } else if !line.is_empty() {
            last_line = line;
            run = 0;
        }
        offset += segment.len();
    }
    match cut_at {
        Some(pos) => std::borrow::Cow::Owned(text[..pos].to_string()),
        None => std::borrow::Cow::Borrowed(text),
    }
}

fn strip_trailing_stray_tokens(before: &str) -> &str {
    let mut end = before.len();
    loop {
        let bytes = before.as_bytes();
        // First skips the trailing newline, then locates.“mostafteroneline”the real end position
        let mut e = end;
        while e > 0 && (bytes[e - 1] == b'\n' || bytes[e - 1] == b'\r') {
            e -= 1;
        }
        let line_start = before[..e].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let last_line = before[line_start..e].trim();
        // Opus When the long context degrades, the leaked <invoke> there is often an isolated one before stray token line.
        // appeared in the measured samples call / count / card three kinds; using a set makes future extension easy.
        if STRAY_INVOKE_TOKENS.contains(&last_line) {
            // only strip stray token the line itself, keeps the newline at the end of the previous line.
            // oldimplementuse line_start - 1 swallowing the previous line newline too would combine the preceding narrative body with
            // subsequent <invoke> squeezed onto the same line, causing invoke_looks_like_real_leak of“line start”decide
            // failure, missing a real leak (narrative\ncall\n<invoke>).changeinto end = line_start:
            //   "some text\ncall" -> "some text\n"(the line start signal is preserved)
            //   "call"(no leading body text)-> ""(line_start==0)
            end = line_start;
            if end == 0 {
                return "";
            }
        } else {
            break;
        }
    }
    &before[..end]
}

/// decidea `<invoke>` blocktobottom looks like“the tool call of a real leak”or still“the text discussed in the body”
///
/// measuredreal leakof `<invoke>` all appearin**line start**(before it is the stream start, or the previous line already ended with a newline),
/// while the one in the body discussion `<invoke>` generally**embedded in the middle of a sentence**——There is still ordinary text before it on the same line.
///
/// determination rule (input `before` is `<invoke>` ofbefore,alreadystripped stray token oftext):
/// - `before` is empty (`<invoke>` instreamstart)→ looks like a real leak, capture.
/// - `before` removetailspace/after a tab it ends with a newline (`<invoke>` aloneoccupynewline)→ capture.
/// - Otherwise (the same line still has non whitespace body before it)→ looks like discussion text, do not capture.
///
/// note:thisinside“tailemptyblank”only strip inline whitespace (space / tab), do not strip newlines;
/// swaplineonly at the endis“start anotherline”the signal.
fn invoke_looks_like_real_leak(before: &str) -> bool {
    // Strips the trailing inline whitespace (spaces / tab), but keep newlines
    let trimmed = before.trim_end_matches([' ', '\t']);
    // Line start: either there is nothing before, or the previous line already ended with a newline.
    trimmed.is_empty() || trimmed.ends_with('\n') || trimmed.ends_with('\r')
}

/// Advances the code fence parity state; for content split across multiple chunk of ``` separator robustness.
///
/// Only when a newline is encountered does it judge whether the reassembled complete line is a fence line (after trimming leading whitespace, starting with ``` at the start).
/// the tail that has not met a newline stays in `partial` in,etc.subsequent chunk align together——so even if ``` split into
/// `` `` `` + `` ` `` two chunk, after reassembling into a complete line it can still flip correctly. `open`.
///
/// The return value is used only internally; the main side effect is updating. `open` and `partial`.
fn advance_code_fence_state(open: &mut bool, partial: &mut String, text: &str) {
    for ch in text.chars() {
        if ch == '\n' {
            if partial.trim_start().starts_with("```") {
                *open = !*open;
            }
            partial.clear();
        } else {
            partial.push(ch);
        }
    }
}

/// Pure function: without changing the real state, trial computes putting `text` whether the fence is open after processing.
/// used for drain at the decision point judge a certain `<invoke>` whether it falls within the fence.
fn fence_open_after(open: bool, partial: &str, text: &str) -> bool {
    let mut o = open;
    let mut p = partial.to_string();
    advance_code_fence_state(&mut o, &mut p, text);
    // must also consider:partial the leftover incomplete line, if it is itself already ``` at start,
    // It does not count as flipped before a newline (conservative: only a complete line flips). Here it returns the already flipped o.
    o
}

/// compute the end of the buffer“may bepart `<invoke` opening tagprefix”bytes; needs to be kept while waiting for more content.
///
/// for examplebufferto `<inv` / `<` / `<i` At the end, it may be a cut apart invoke opening tag,
/// keep this tail segment for the next chunk Assembles fully, avoiding emitting half a tag as text.
fn partial_invoke_tag_suffix_len(buf: &str) -> usize {
    // any form like `<...`(mostaftera '<' afterwards none '>') the tail may be a partial open tag.
    if let Some(lt) = buf.rfind('<') {
        if !buf[lt..].contains('>') {
            return buf.len() - lt;
        }
    }
    0
}

/// extract from the complete text thinking block (used for non streaming response)
///
/// Uses the same tag detection logic as streaming (quote character filtering) to ensure consistency.
/// In the non streaming case the text is complete; no need to handle cross chunk splitissue.
///
/// # return value
/// - `(Some(thinking_content), remaining_text)` — detectedhaseffect thinking block
/// - `(None, original_text)` — not detected, return as is
pub(crate) fn extract_thinking_from_complete_text(text: &str) -> (Option<String>, String) {
    let start_pos = match find_real_thinking_start_tag(text) {
        Some(pos) => pos,
        None => return (None, text.to_string()),
    };

    let before = &text[..start_pos];
    let after_open = &text[start_pos + "<thinking>".len()..];

    // Finds the end tag: prefers matching the one with \n\n the suffix, falling back to an end match.
    let (thinking_raw, text_after) = if let Some(end_pos) = find_real_thinking_end_tag(after_open) {
        (
            &after_open[..end_pos],
            &after_open[end_pos + "</thinking>\n\n".len()..],
        )
    } else if let Some(end_pos) = find_real_thinking_end_tag_at_buffer_end(after_open) {
        let after_tag = end_pos + "</thinking>".len();
        (&after_open[..end_pos], after_open[after_tag..].trim_start())
    } else {
        // No valid end tag found; does not extract.
        return (None, text.to_string());
    };

    // Strips leading newlines (consistent with streaming: model output <thinking>\n)
    let thinking_content = thinking_raw.strip_prefix('\n').unwrap_or(thinking_raw);

    // Assembles the remaining text: skips the pure whitespace ones. before part
    let mut remaining = String::new();
    if !before.trim().is_empty() {
        remaining.push_str(before);
    }
    remaining.push_str(text_after);

    if thinking_content.is_empty() {
        (None, remaining)
    } else {
        (Some(thinking_content.to_string()), remaining)
    }
}

/// at once (non streaming / the whole segment is complete) take assistant split text into Anthropic content block sequence,
/// take the literal mixed into the text `<invoke name="...">...</invoke>` recover the tool call into a structured `tool_use`.
///
/// reusewith streaming `drain_invoke_sniff_buffer` **exactly the same**safety judgment, avoiding wrongly catching commands discussed in the body:
///   ① line start check `invoke_looks_like_real_leak`(remove before block stray token aftermustinline start)
///   ② code fencedecide `fence_open_after`(by ``` wrapped display text is not recovered)
///   ③ tool tablehard guardrail `known_tool_names`(the parsed tool name must be a tool declared by this request)
/// any onenotsatisfy → this `<invoke>` The block is kept as is as plain text.
///
/// The difference from the streaming version: here the input is**already complete**the whole segment of text, so no need to hold buffer,
/// partopening tag,`MAX_INVOKE_HOLD_BYTES` that incremental logic——a direct linear scan suffices.
///
/// returned content block The form is consistent with the caller existing convention:
///   - text:`{"type":"text","text": "..."}`
///   - tool:`{"type":"tool_use","id":"toolu_...","name":"...","input": {...}}`
/// Text blocks merge adjacent pieces on demand; empty text pieces are not produced.`input` parsefailedwhen fall back into `{}`.
///
/// `tool_name_map`(short name → original name) used to restore the recovered tool name back to the original name the client recognizes,
/// with streaming `synthesize_tool_use` consistent; when the mapping is empty or no hit, returns the original name.
pub(crate) fn extract_invoke_content_blocks(
    text: &str,
    known_tool_names: &std::collections::HashSet<String>,
    tool_name_map: &std::collections::HashMap<String, String>,
) -> Vec<serde_json::Value> {
    // 🛑 block level repeat readout circuit breaker: first Opus degradeofopen bracketsame stray token consecutive repeat readout truncation,
    // then do invoke sniff.override web_search loop(99.9% real traffic) this non streaming path.
    let collapsed = collapse_stray_token_floods(text);
    let text: &str = &collapsed;
    let mut blocks: Vec<serde_json::Value> = Vec::new();
    let mut pending_text = String::new();
    // Fence parity state: advances across the already emitted text, ensuring ``` can be correctly determined across fragments.
    let mut fence_open = false;
    let mut fence_partial = String::new();

    let push_text = |blocks: &mut Vec<serde_json::Value>, pending: &mut String| {
        if !pending.is_empty() {
            blocks.push(serde_json::json!({"type": "text", "text": pending.clone()}));
            pending.clear();
        }
    };

    let mut rest = text;
    loop {
        let start = match find_invoke_start(rest) {
            Some(s) => s,
            None => {
                pending_text.push_str(rest);
                break;
            }
        };
        let end = match find_invoke_block_end(rest, start) {
            Some(e) => e,
            None => {
                // The block is not closed (the whole segment is complete yet still no </invoke>)→ Not a clean tool call; the whole segment is treated as text.
                pending_text.push_str(rest);
                break;
            }
        };

        let before = &rest[..start];
        let stripped_before = strip_trailing_stray_tokens(before);
        // ③ Fence: whether the fence is open after the text before the block is processed.
        let fence_after_before = fence_open_after(fence_open, &fence_partial, before);
        // ② toolnameparse + tool tableguardrail
        let parsed = parse_invoke_block(&rest[start..end]);
        let name_known = parsed
            .as_ref()
            .map(|(n, _)| known_tool_names.contains(n))
            .unwrap_or(false);

        if invoke_looks_like_real_leak(stripped_before) && !fence_after_before && name_known {
            // real leak: keep the stripped stray token the preceding text (advances the fence), then produces structured tool_use.
            if !stripped_before.is_empty() {
                advance_code_fence_state(&mut fence_open, &mut fence_partial, stripped_before);
                pending_text.push_str(stripped_before);
            }
            push_text(&mut blocks, &mut pending_text);
            let (name, input_json) = parsed.expect("parsed is Some when name_known");
            let input: serde_json::Value =
                serde_json::from_str(&input_json).unwrap_or_else(|_| serde_json::json!({}));
            // Restore the original (client-facing) tool name: long names (>63) are shortened
            // before being sent upstream, so the model may leak the SHORT name. The host
            // matches on the original name — mirror synthesize_tool_use's restoration.
            let name = tool_name_map.get(&name).cloned().unwrap_or(name);
            let tool_use_id = format!("toolu_{}", Uuid::new_v4().to_string().replace('-', ""));
            blocks.push(serde_json::json!({
                "type": "tool_use",
                "id": tool_use_id,
                "name": name,
                "input": input,
            }));
        } else {
            // notretrieve(sentencein / inside fence / toolnameunknown / parsefailed)→ whole block (including before) as text, advance the fence.
            let chunk = &rest[..end];
            advance_code_fence_state(&mut fence_open, &mut fence_partial, chunk);
            pending_text.push_str(chunk);
        }
        rest = &rest[end..];
    }

    push_text(&mut blocks, &mut pending_text);
    blocks
}

/// SSE event
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: String,
    pub data: serde_json::Value,
}

impl SseEvent {
    pub fn new(event: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            event: event.into(),
            data,
        }
    }

    /// formatted as SSE string
    pub fn to_sse_string(&self) -> String {
        format!(
            "event: {}\ndata: {}\n\n",
            self.event,
            serde_json::to_string(&self.data).unwrap_or_default()
        )
    }
}

/// content blockstate
#[derive(Debug, Clone)]
struct BlockState {
    block_type: String,
    started: bool,
    stopped: bool,
}

impl BlockState {
    fn new(block_type: impl Into<String>) -> Self {
        Self {
            block_type: block_type.into(),
            started: false,
            stopped: false,
        }
    }
}

/// SSE statemanager
///
/// ensure SSE event sequenceconform to Claude API spec:
/// 1. message_start onlycanappearonce
/// 2. content_block must first start again delta again stop
/// 3. message_delta can appear only once, and among all content_block_stop after
/// 4. message_stop at the end
#[derive(Debug)]
pub struct SseStateManager {
    /// message_start iswhetheralreadysend
    message_started: bool,
    /// message_delta iswhetheralreadysend
    message_delta_sent: bool,
    /// the active content block state
    active_blocks: HashMap<i32, BlockState>,
    /// whether the message has ended
    message_ended: bool,
    /// nextblock index
    next_block_index: i32,
    /// current stop_reason
    stop_reason: Option<String>,
    /// whether there is a tool call
    has_tool_use: bool,
}

impl Default for SseStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SseStateManager {
    pub fn new() -> Self {
        Self {
            message_started: false,
            message_delta_sent: false,
            active_blocks: HashMap::new(),
            message_ended: false,
            next_block_index: 0,
            stop_reason: None,
            has_tool_use: false,
        }
    }

    /// Determines whether the given block is in a receivable delta ofopenstate
    fn is_block_open_of_type(&self, index: i32, expected_type: &str) -> bool {
        self.active_blocks
            .get(&index)
            .is_some_and(|b| b.started && !b.stopped && b.block_type == expected_type)
    }

    /// get the next block index
    pub fn next_block_index(&mut self) -> i32 {
        let index = self.next_block_index;
        self.next_block_index += 1;
        index
    }

    /// recordtool call
    pub fn set_has_tool_use(&mut self, has: bool) {
        self.has_tool_use = has;
    }

    /// set stop_reason
    pub fn set_stop_reason(&mut self, reason: impl Into<String>) {
        self.stop_reason = Some(reason.into());
    }

    /// check whether there exists a non thinking content block of the type (such as text or tool_use)
    fn has_non_thinking_blocks(&self) -> bool {
        self.active_blocks
            .values()
            .any(|b| b.block_type != "thinking")
    }

    /// fetchfinalof stop_reason
    pub fn get_stop_reason(&self) -> String {
        if let Some(ref reason) = self.stop_reason {
            reason.clone()
        } else if self.has_tool_use {
            "tool_use".to_string()
        } else {
            "end_turn".to_string()
        }
    }

    /// handle message_start event
    pub fn handle_message_start(&mut self, event: serde_json::Value) -> Option<SseEvent> {
        if self.message_started {
            tracing::debug!("skipduplicate message_start event");
            return None;
        }
        self.message_started = true;
        Some(SseEvent::new("message_start", event))
    }

    /// handle content_block_start event
    pub fn handle_content_block_start(
        &mut self,
        index: i32,
        block_type: &str,
        data: serde_json::Value,
    ) -> Vec<SseEvent> {
        let mut events = Vec::new();

        // if it is tool_use block, first closes the previous text block.
        if block_type == "tool_use" {
            self.has_tool_use = true;
            for (block_index, block) in self.active_blocks.iter_mut() {
                if block.block_type == "text" && block.started && !block.stopped {
                    // auto send content_block_stop closetext block
                    events.push(SseEvent::new(
                        "content_block_stop",
                        json!({
                            "type": "content_block_stop",
                            "index": block_index
                        }),
                    ));
                    block.stopped = true;
                }
            }
        }

        // check whether the block already exists
        if let Some(block) = self.active_blocks.get_mut(&index) {
            if block.started {
                tracing::debug!("block {} already started, skip the duplicate content_block_start", index);
                return events;
            }
            block.started = true;
        } else {
            let mut block = BlockState::new(block_type);
            block.started = true;
            self.active_blocks.insert(index, block);
        }

        events.push(SseEvent::new("content_block_start", data));
        events
    }

    /// handle content_block_delta event
    pub fn handle_content_block_delta(
        &mut self,
        index: i32,
        data: serde_json::Value,
    ) -> Option<SseEvent> {
        // ensureblockalreadystart
        if let Some(block) = self.active_blocks.get(&index) {
            if !block.started || block.stopped {
                tracing::warn!(
                    "block {} abnormal state: started={}, stopped={}",
                    index,
                    block.started,
                    block.stopped
                );
                return None;
            }
        } else {
            // The block does not exist; it may need to be created first.
            tracing::warn!("receivedunknownblock {} of delta event", index);
            return None;
        }

        Some(SseEvent::new("content_block_delta", data))
    }

    /// handle content_block_stop event
    pub fn handle_content_block_stop(&mut self, index: i32) -> Option<SseEvent> {
        if let Some(block) = self.active_blocks.get_mut(&index) {
            if block.stopped {
                tracing::debug!("block {} already stopped, skip the duplicate content_block_stop", index);
                return None;
            }
            block.stopped = true;
            return Some(SseEvent::new(
                "content_block_stop",
                json!({
                    "type": "content_block_stop",
                    "index": index
                }),
            ));
        }
        None
    }

    /// generate the final event sequence
    pub fn generate_final_events(
        &mut self,
        input_tokens: i32,
        output_tokens: i32,
        cache_creation_input_tokens: i32,
        cache_read_input_tokens: i32,
    ) -> Vec<SseEvent> {
        let mut events = Vec::new();

        // close all unclosed blocks
        for (index, block) in self.active_blocks.iter_mut() {
            if block.started && !block.stopped {
                events.push(SseEvent::new(
                    "content_block_stop",
                    json!({
                        "type": "content_block_stop",
                        "index": index
                    }),
                ));
                block.stopped = true;
            }
        }

        // send message_delta
        if !self.message_delta_sent {
            self.message_delta_sent = true;
            events.push(SseEvent::new(
                "message_delta",
                json!({
                    "type": "message_delta",
                    "delta": {
                        "stop_reason": self.get_stop_reason(),
                        "stop_sequence": null
                    },
                    "usage": {
                        "input_tokens": input_tokens,
                        "output_tokens": output_tokens,
                        "cache_creation_input_tokens": cache_creation_input_tokens,
                        "cache_read_input_tokens": cache_read_input_tokens
                    }
                }),
            ));
        }

        // send message_stop
        if !self.message_ended {
            self.message_ended = true;
            events.push(SseEvent::new(
                "message_stop",
                json!({ "type": "message_stop" }),
            ));
        }

        events
    }
}

use super::converter::get_context_window_size;

/// streamhandlecontext
pub struct StreamContext {
    /// SSE statemanager
    pub state_manager: SseStateManager,
    /// the requested model name
    pub model: String,
    /// message ID
    pub message_id: String,
    /// input tokens(estimatevalue)
    pub input_tokens: i32,
    /// from contextUsageEvent the computed actual input tokens
    pub context_input_tokens: Option<i32>,
    /// output tokens cumulative
    pub output_tokens: i32,
    /// tool block index mapping (tool_id -> block_index)
    pub tool_block_indices: HashMap<String, i32>,
    /// Tool name reverse mapping (short name → original name), used to restore during the response.
    pub tool_name_map: HashMap<String, String>,
    /// All tool names declared by this request (original client name).`<invoke>` disaster fallback for text fault tolerance:
    /// Only when the synthesized name is in this set is recovery into structured allowed. tool_use, otherwise emit as text.
    /// is empty (the request did not carry tools) do not recover anything invoke——Better to miss than to wrongly execute.
    pub known_tool_names: std::collections::HashSet<String>,
    /// Code fence parity state across the whole stream: each time a line is encountered starting with ``` startthen flip.
    /// ininside fence(true) when,`<invoke>` Never recovered (treated as a code block shown in the body).
    pub code_fence_open: bool,
    /// Accumulator for the incomplete line in fence detection: only on a newline does it judge whether the complete line is ``` fence line.
    /// so even if ``` the separator is split into multiple chunk(such as `` `` + ` ``), after reassembling into a complete line it can still be correctly recognized.
    pub fence_scan_partial: String,
    /// thinking iswhetherenable
    pub thinking_enabled: bool,
    /// thinking contentbuffer
    pub thinking_buffer: String,
    /// invoke Text sniff buffer (used to sniff literals from the plaintext stream). `<invoke>` tool callblock)
    pub invoke_sniff_buffer: String,
    /// whether in thinking within block
    pub in_thinking_block: bool,
    /// thinking whether the block has finished extraction
    pub thinking_extracted: bool,
    /// thinking block index
    pub thinking_block_index: Option<i32>,
    /// upstream native reasoningContentEvent dispatched thinking signature
    pending_thinking_signature: Option<String>,
    /// text blockindex (thinking dynamically allocate when enabled)
    pub text_block_index: Option<i32>,
    /// iswhetherneedstrip thinking the newline at the start of the content
    /// model output `<thinking>\n` when,`\n` may be on the same as the tag chunk or next chunk
    strip_thinking_leading_newline: bool,
    /// relay layer CacheMeter the cache coverage (estimate basis). At final report, by the real total
    /// perform mutually exclusive allocation:`input + cache_creation + cache_read == total`, to avoid treating the cached
    /// the covered prefix is counted repeatedly into input_tokens.
    pub cache_usage: super::cache_metering::CacheUsage,
    /// meteringEvent reported credit Billing amount (truly delivered by upstream).
    pub credits: f64,
    /// Repeat circuit breaker: the content of the most recent tail line emitted as text (whitespace trimmed).
    /// Opus When the long context degrades it repeats the same one. stray token(call/count/card) repeats line by line infinitely,
    /// At the text exit we count how many times the same short line repeated consecutively.
    repeat_guard_last_line: String,
    /// Repeat circuit breaker: the number of consecutive repeats of the current tail line.
    repeat_guard_run: u32,
    /// Repeat circuit breaker: whether the breaker has already tripped (once tripped, all remaining text this round is dropped, not emitted and not written to history).
    repeat_guard_tripped: bool,
}

impl StreamContext {
    /// parse the final reported measure `(input_tokens, cache_creation, cache_read)`.
    ///
    /// total truthyprioritytake contextUsage(the real upstream percentage×window), otherwise uses the client estimated one.
    /// `input_tokens`; then by [`CacheUsage::split_against_total`] perform mutually exclusive allocation.
    pub fn resolved_usage(&self) -> (i32, i32, i32) {
        let total_real = self.context_input_tokens.unwrap_or(self.input_tokens);
        self.cache_usage.split_against_total(total_real)
    }
    /// create StreamContext
    pub fn new_with_thinking(
        model: impl Into<String>,
        input_tokens: i32,
        thinking_enabled: bool,
        tool_name_map: HashMap<String, String>,
        known_tool_names: std::collections::HashSet<String>,
    ) -> Self {
        Self {
            state_manager: SseStateManager::new(),
            model: model.into(),
            message_id: format!("msg_{}", Uuid::new_v4().to_string().replace('-', "")),
            input_tokens,
            context_input_tokens: None,
            output_tokens: 0,
            tool_block_indices: HashMap::new(),
            tool_name_map,
            known_tool_names,
            code_fence_open: false,
            fence_scan_partial: String::new(),
            thinking_enabled,
            thinking_buffer: String::new(),
            invoke_sniff_buffer: String::new(),
            in_thinking_block: false,
            thinking_extracted: false,
            thinking_block_index: None,
            pending_thinking_signature: None,
            text_block_index: None,
            strip_thinking_leading_newline: false,
            cache_usage: super::cache_metering::CacheUsage::default(),
            credits: 0.0,
            repeat_guard_last_line: String::new(),
            repeat_guard_run: 0,
            repeat_guard_tripped: false,
        }
    }

    /// generate message_start event
    pub fn create_message_start_event(&self) -> serde_json::Value {
        json!({
            "type": "message_start",
            "message": {
                "id": self.message_id,
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": self.model,
                "stop_reason": null,
                "stop_sequence": null,
                "usage": {
                    "input_tokens": self.input_tokens,
                    "output_tokens": 1,
                    "cache_creation_input_tokens": 0,
                    "cache_read_input_tokens": 0
                }
            }
        })
    }

    /// generate the initial event sequence (message_start + text block start)
    ///
    /// when thinking When enabled, does not create a text block at init, but waits until content is actually received before creating it.
    /// this way cantoensure thinking block (index 0) in the text block (index 1) before.
    pub fn generate_initial_events(&mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();

        // message_start
        let msg_start = self.create_message_start_event();
        if let Some(event) = self.state_manager.handle_message_start(msg_start) {
            events.push(event);
        }

        // ifenabledone thinking, do not create a text block here
        // thinking the block and the text block will be at process_content_with_thinking create in the correct order in
        if self.thinking_enabled {
            return events;
        }

        // Creates the initial text block (only when not enabled thinking when)
        let text_block_index = self.state_manager.next_block_index();
        self.text_block_index = Some(text_block_index);
        let text_block_events = self.state_manager.handle_content_block_start(
            text_block_index,
            "text",
            json!({
                "type": "content_block_start",
                "index": text_block_index,
                "content_block": {
                    "type": "text",
                    "text": ""
                }
            }),
        );
        events.extend(text_block_events);

        events
    }

    /// handle Kiro eventandconvert to Anthropic SSE event
    pub fn process_kiro_event(&mut self, event: &Event) -> Vec<SseEvent> {
        match event {
            Event::AssistantResponse(resp) => self.process_assistant_response(&resp.content),
            Event::ToolUse(tool_use) => self.process_tool_use(tool_use),
            Event::ReasoningContent(reasoning) => self.process_reasoning_content(reasoning),
            Event::ContextUsage(context_usage) => {
                // Computes the actual one from the context usage percentage. input_tokens
                let window_size = get_context_window_size(&self.model);
                let actual_input_tokens =
                    (context_usage.context_usage_percentage * (window_size as f64) / 100.0) as i32;
                self.context_input_tokens = Some(actual_input_tokens);
                // the context usage reaches 100% when,set stop_reason as model_context_window_exceeded
                if context_usage.context_usage_percentage >= 100.0 {
                    self.state_manager
                        .set_stop_reason("model_context_window_exceeded");
                }
                tracing::debug!(
                    "received contextUsageEvent: {}%, compute input_tokens: {}",
                    context_usage.context_usage_percentage,
                    actual_input_tokens
                );
                Vec::new()
            }
            Event::Metering(metering) => {
                // upstream meteringEvent only dispatch credit;token / cache fielddoes not exist.
                self.credits += metering.usage;
                tracing::debug!("metering credits +{:.6}", metering.usage);
                Vec::new()
            }
            Event::Error {
                error_code,
                error_message,
            } => {
                tracing::error!("receivederrorevent: {} - {}", error_code, error_message);
                Vec::new()
            }
            Event::Exception {
                exception_type,
                message,
            } => {
                // handle ContentLengthExceededException
                if exception_type == "ContentLengthExceededException" {
                    self.state_manager.set_stop_reason("max_tokens");
                }
                tracing::warn!("receivedexceptionevent: {} - {}", exception_type, message);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// handle the assistant response event
    fn process_assistant_response(&mut self, content: &str) -> Vec<SseEvent> {
        if content.is_empty() {
            return Vec::new();
        }

        let mut events = Vec::new();
        if self.is_thinking_block_open() && !self.in_thinking_block {
            events.extend(self.close_open_thinking_block());
        }

        // estimate tokens
        self.output_tokens += estimate_tokens(content);

        // ifenabledonethinking,needhandlethinkingblock
        if self.thinking_enabled {
            events.extend(self.process_content_with_thinking(content));
            return events;
        }

        // non thinking the mode also reuses the unified text_delta sendlogic,
        // so that in tool_use After auto closing the text block, can self heal by rebuilding a new text block, avoiding“swallow characters”.
        events.extend(self.create_text_delta_events(content));
        events
    }

    /// handle containingthinkingblock content
    fn process_content_with_thinking(&mut self, content: &str) -> Vec<SseEvent> {
        let mut events = Vec::new();

        // Adds the content to the buffer for processing.
        self.thinking_buffer.push_str(content);

        loop {
            if !self.in_thinking_block && !self.thinking_extracted {
                // find <thinking> Start tag (skips those wrapped by backticks).
                if let Some(start_pos) = find_real_thinking_start_tag(&self.thinking_buffer) {
                    // send <thinking> the preceding content as text_delta
                    // Note: if what precedes is only whitespace (such as adaptive modereturned \n\n),thenskip,
                    // avoid in thinking produces meaningless before the block text block causes the client to fail parsing
                    let before_thinking = self.thinking_buffer[..start_pos].to_string();
                    if !before_thinking.is_empty() && !before_thinking.trim().is_empty() {
                        events.extend(self.create_text_delta_events(&before_thinking));
                    }

                    // enter thinking block
                    self.in_thinking_block = true;
                    self.strip_thinking_leading_newline = true;
                    self.thinking_buffer =
                        self.thinking_buffer[start_pos + "<thinking>".len()..].to_string();

                    // create thinking block content_block_start event
                    let thinking_index = self.state_manager.next_block_index();
                    self.thinking_block_index = Some(thinking_index);
                    let start_events = self.state_manager.handle_content_block_start(
                        thinking_index,
                        "thinking",
                        json!({
                            "type": "content_block_start",
                            "index": thinking_index,
                            "content_block": {
                                "type": "thinking",
                                "thinking": ""
                            }
                        }),
                    );
                    events.extend(start_events);
                } else {
                    // not found <thinking>, checks whether it may be a partial tag.
                    // Keeps content that may be a partial tag.
                    let target_len = self
                        .thinking_buffer
                        .len()
                        .saturating_sub("<thinking>".len());
                    let safe_len = find_char_boundary(&self.thinking_buffer, target_len);
                    if safe_len > 0 {
                        let safe_content = self.thinking_buffer[..safe_len].to_string();
                        // if thinking not yet extracted, and the safe content is only whitespace,
                        // thennotsendas text_delta, keeps it in the buffer waiting for more content.
                        // this avoids 4.6 in model <thinking> when the tag is split across events,
                        // beforeguideemptyblank(such as "\n\n") was wrongly created as text block,
                        // cause text block precedes thinking the problem of the block appearing.
                        if !safe_content.is_empty() && !safe_content.trim().is_empty() {
                            events.extend(self.create_text_delta_events(&safe_content));
                            self.thinking_buffer = self.thinking_buffer[safe_len..].to_string();
                        }
                    }
                    break;
                }
            } else if self.in_thinking_block {
                // strip <thinking> The newline immediately after the tag (may span chunk)
                if self.strip_thinking_leading_newline {
                    if self.thinking_buffer.starts_with('\n') {
                        self.thinking_buffer = self.thinking_buffer[1..].to_string();
                        self.strip_thinking_leading_newline = false;
                    } else if !self.thinking_buffer.is_empty() {
                        // buffer nonemptybutnotto \n at the start, no longer need to strip
                        self.strip_thinking_leading_newline = false;
                    }
                    // buffer When empty, keeps the flag and waits for the next one. chunk
                }

                // in thinking within block,find </thinking> End tag (skips those wrapped by backticks).
                if let Some(end_pos) = find_real_thinking_end_tag(&self.thinking_buffer) {
                    // extract thinking content
                    let thinking_content = self.thinking_buffer[..end_pos].to_string();
                    if !thinking_content.is_empty() {
                        if let Some(thinking_index) = self.thinking_block_index {
                            events.push(
                                self.create_thinking_delta_event(thinking_index, &thinking_content),
                            );
                        }
                    }

                    // end thinking block
                    self.in_thinking_block = false;
                    self.thinking_extracted = true;

                    // send empty thinking_delta event, then send content_block_stop event
                    if let Some(thinking_index) = self.thinking_block_index {
                        // firstsend empty thinking_delta
                        events.push(self.create_thinking_delta_event(thinking_index, ""));
                        // signature_delta:satisfyclient thinking local validation under the mode
                        events.push(self.create_signature_delta_event(thinking_index));
                        // then send content_block_stop
                        if let Some(stop_event) =
                            self.state_manager.handle_content_block_stop(thinking_index)
                        {
                            events.push(stop_event);
                        }
                    }

                    // strip `</thinking>\n\n`(find_real_thinking_end_tag confirmed \n\n exists)
                    self.thinking_buffer =
                        self.thinking_buffer[end_pos + "</thinking>\n\n".len()..].to_string();
                } else {
                    // No end tag was found; sends the current buffer content as thinking_delta.
                    // keep the tail which may be partial `</thinking>\n\n` content:
                    // find_real_thinking_end_tag requiretagafterhas `\n\n` return only then Some,
                    // therefore the retained region must cover `</thinking>\n\n` ofcompletelength(13 bytes),
                    // otherwise when `</thinking>` already in buffer but `\n\n` not yetarrivewhen,
                    // The first few characters of the tag would be wrongly taken as thinking_delta emit.
                    let target_len = self
                        .thinking_buffer
                        .len()
                        .saturating_sub("</thinking>\n\n".len());
                    let safe_len = find_char_boundary(&self.thinking_buffer, target_len);
                    if safe_len > 0 {
                        let safe_content = self.thinking_buffer[..safe_len].to_string();
                        if !safe_content.is_empty() {
                            if let Some(thinking_index) = self.thinking_block_index {
                                events.push(
                                    self.create_thinking_delta_event(thinking_index, &safe_content),
                                );
                            }
                        }
                        self.thinking_buffer = self.thinking_buffer[safe_len..].to_string();
                    }
                    break;
                }
            } else {
                // thinking Already extracted; the remaining content serves as text_delta
                if !self.thinking_buffer.is_empty() {
                    let remaining = self.thinking_buffer.clone();
                    self.thinking_buffer.clear();
                    events.extend(self.create_text_delta_events(&remaining));
                }
                break;
            }
        }

        events
    }

    /// create text_delta event (with invoke the unified plaintext funnel for sniffing)
    ///
    /// this is thinking / non thinking two paths + The only plaintext exit shared by the two endpoints.
    /// accumulate the text here into `invoke_sniff_buffer`, loop sniff the complete literal `<invoke>` tool callblock:
    /// - Hit a complete block: first take the text before the block (strip the trailing standalone `call`/`count` line) goes `emit_text_delta_raw` emit,
    ///   againsynthesizestructtransform tool_use the event, then continue the loop;
    /// - No complete block hit: keeps the possible partial tag tail in the buffer, the rest goes to `emit_text_delta_raw`.
    fn create_text_delta_events(&mut self, text: &str) -> Vec<SseEvent> {
        if text.is_empty() {
            return Vec::new();
        }
        self.invoke_sniff_buffer.push_str(text);
        self.drain_invoke_sniff_buffer(false)
    }

    /// line startnot closed `<invoke` The byte limit of the block. Only used to prevent"one at the line start that never closes `<invoke`
    /// takewholeentrystreampermanent hold hold"this extreme case; the normal invoke(even ifislarge patch) are all far smaller than this,
    /// so it does not wrongly kill a legitimate multi line one./fragmented tool call.
    const MAX_INVOKE_HOLD_BYTES: usize = 262_144;

    /// sniff and arrangeempty `invoke_sniff_buffer`
    ///
    /// - `flush=false`(mid stream): when no complete block is hit, keeps the tail that may be a partial tag (at most one unclosed
    ///   `<invoke` block or a suspected open tag prefix), the remaining prefix text goes to `emit_text_delta_raw` emit.
    /// - `flush=true`(stream end): no longer keeps a tail, all remaining goes to `emit_text_delta_raw` emit (to prevent trailing byte loss).
    fn drain_invoke_sniff_buffer(&mut self, flush: bool) -> Vec<SseEvent> {
        let mut events = Vec::new();
        // Drive the loop on an owned local buffer taken out of `self` ONCE, instead of
        // cloning `self.invoke_sniff_buffer` on every iteration. Under degraded-model
        // floods this buffer can grow up to MAX_INVOKE_HOLD_BYTES, so a per-iteration
        // full clone was O(n) per loop (quadratic overall). The only in-loop allocation
        // now is the (smaller) remainder after a reclaimed block. Every exit path writes
        // the intended remainder back into `self.invoke_sniff_buffer` (empty if fully
        // consumed); the Some->Some path keeps looping on the local `buf`.
        let mut buf = std::mem::take(&mut self.invoke_sniff_buffer);
        loop {
            match find_invoke_start(&buf) {
                Some(start) => {
                    match find_invoke_block_end(&buf, start) {
                        Some(end) => {
                            // Hit a complete block: first judge whether it looks like a real leak or body discussion (P1 ambiguous signalnumber)
                            let before = strip_trailing_stray_tokens(&buf[..start]);
                            // 🅱 first take before the fence open and close there merge into a trial state: if this <invoke>
                            // Falls within a code fence (a code block shown in the body); never recovered, emitted as text.
                            let fence_after_before = fence_open_after(
                                self.code_fence_open,
                                &self.fence_scan_partial,
                                before,
                            );
                            // 🅳 Disaster fallback: only when the parsed tool name is in the tool table declared by this request is recovery allowed.
                            // the table is empty (the request did not carry tools) or the name is not in the table → Emitted as text; better to miss than to wrongly execute.
                            let parsed = parse_invoke_block(&buf[start..end]);
                            let name_known = parsed
                                .as_ref()
                                .map(|(n, _)| self.known_tool_names.contains(n))
                                .unwrap_or(false);
                            if invoke_looks_like_real_leak(before) && !fence_after_before && name_known {
                                // Real leak: emits the text before the block (strips the trailing standalone call/count line)+ synthesize tool_use
                                if !before.is_empty() {
                                    events.extend(self.emit_text_delta_raw(before));
                                }
                                // parsed already confirmed above to be Some and name_known
                                let (name, input_json) = parsed.expect("parsed is Some when name_known");
                                events.extend(self.synthesize_tool_use(name, input_json));
                            } else {
                                // do not recover (embedded in a sentence / inside fence / toolnameunknown / parsefailed)→ emit the whole segment as ordinary text
                                events.extend(self.emit_text_delta_raw(&buf[..end]));
                            }
                            // Advances the local buffer past the block and continues the loop (no longer writes back self, notagainoverall clone)
                            buf = buf[end..].to_string();
                            continue;
                        }
                        None => {
                            // the block has not fully arrived. use first P1 line start determination: not at the line start <invoke whendiscusstext,
                            // emit the whole segment directly, do not enter hold buffer (P2: avoid hold the subsequent text to the stream end).
                            let before = strip_trailing_stray_tokens(&buf[..start]);
                            // 🅱 the unclosed within the fence <invoke> also not hold(it is a body code block), emitted directly as text.
                            let fence_after_before = fence_open_after(
                                self.code_fence_open,
                                &self.fence_scan_partial,
                                before,
                            );
                            if !invoke_looks_like_real_leak(before) || fence_after_before {
                                if !buf.is_empty() {
                                    events.extend(self.emit_text_delta_raw(&buf));
                                }
                                break;
                            }
                            // the unclosed block at the line start: take start emit the preceding text, keep start.. wait for close
                            if start > 0 {
                                events.extend(self.emit_text_delta_raw(&buf[..start]));
                            }
                            let remainder = buf[start..].to_string();
                            if flush {
                                // flush Mode: a leftover half block is emitted as plain text.
                                if !remainder.is_empty() {
                                    events.extend(self.emit_text_delta_raw(&remainder));
                                }
                            } else {
                                // P2 limit:hold of <invoke The block accumulation exceeds the threshold yet still does not wait for </invoke>,
                                // Gives up waiting and emits as plain text, avoiding indefinite hold subsequenttext.
                                // use only a pure byte upper limit as fallback"forevernotclosedof `<invoke` stall the stream";
                                // no longer give up by the newline count——multi lineparameter(apply_patch etc.)isnormal state,
                                // the newline count is not the give up hold a good signal; otherwise it would wrongly kill a legitimate one arriving in pieces. invoke.
                                let too_long = remainder.len() > Self::MAX_INVOKE_HOLD_BYTES;
                                if too_long {
                                    events.extend(self.emit_text_delta_raw(&remainder));
                                } else {
                                    // retainhalfblockto self, wait for the next fragment to arrive then continue
                                    self.invoke_sniff_buffer = remainder;
                                }
                            }
                            break;
                        }
                    }
                }
                None => {
                    // noneany invoke opening tag
                    if flush {
                        if !buf.is_empty() {
                            events.extend(self.emit_text_delta_raw(&buf));
                        }
                    } else {
                        // keep a segment that may be partial `<invoke` the tail of the open tag prefix; the rest is emitted.
                        let keep = partial_invoke_tag_suffix_len(&buf);
                        let split = buf.len() - keep;
                        let safe = find_char_boundary(&buf, split);
                        if safe > 0 {
                            events.extend(self.emit_text_delta_raw(&buf[..safe]));
                        }
                        self.invoke_sniff_buffer = buf[safe..].to_string();
                    }
                    break;
                }
            }
        }
        events
    }

    /// synthesize a set of structured tool_use event (copy verbatim process_tool_use of 6 step)
    fn synthesize_tool_use(&mut self, parsed_name: String, input_json: String) -> Vec<SseEvent> {
        let mut events = Vec::new();
        self.state_manager.set_has_tool_use(true);
        let block_index = self.state_manager.next_block_index();
        let tool_use_id = format!("toolu_{}", Uuid::new_v4().to_string().replace('-', ""));
        self.tool_block_indices
            .insert(tool_use_id.clone(), block_index);
        let name = self
            .tool_name_map
            .get(&parsed_name)
            .cloned()
            .unwrap_or(parsed_name);
        events.extend(self.state_manager.handle_content_block_start(
            block_index,
            "tool_use",
            json!({
                "type": "content_block_start",
                "index": block_index,
                "content_block": {
                    "type": "tool_use",
                    "id": tool_use_id,
                    "name": name,
                    "input": {}
                }
            }),
        ));
        if let Some(d) = self.state_manager.handle_content_block_delta(
            block_index,
            json!({
                "type": "content_block_delta",
                "index": block_index,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": input_json
                }
            }),
        ) {
            events.push(d);
        }
        if let Some(s) = self.state_manager.handle_content_block_stop(block_index) {
            events.push(s);
        }
        events
    }

    /// create text_delta event (original logic, no sniffing).
    ///
    /// If the text block has not been created yet, creates it first.
    /// when occurs tool_use the state machine automatically closes the current text block; subsequent text automatically creates a new text block and continues output.
    ///
    /// the return value contains the possible content_block_start event and content_block_delta event.
    /// Repeat circuit breaker filter: before text is actually emitted to the client, checks line by line for the same stray token consecutiverepeat.
    ///
    /// Works this way (stream safe, cross chunk cumulative):
    /// - take incoming `text` Splits by line and compares each line with the previous (whitespace trimmed);
    /// - only for `STRAY_INVOKE_TOKENS`(call/count/card) such degraded guide word counts; ordinary text is always passed;
    /// - same stray token consecutiveduplicatereachto `REPEAT_GUARD_TRIP_THRESHOLD` that is tripping the breaker;
    /// - After tripping: any subsequent text this round (including continued repeats count) are all discarded, returns an empty string.
    ///
    /// Returns the text that should continue to be emitted (returns an empty string when tripped).
    fn repeat_guard_filter(&mut self, text: &str) -> String {
        // Already tripped: all remaining text this round is discarded, breaking the snowball.
        if self.repeat_guard_tripped {
            return String::new();
        }

        let mut kept = String::new();
        // use split_inclusive Keeps newlines to ensure passed normal text does not lose bytes.
        for segment in text.split_inclusive('\n') {
            let line = segment.trim();
            if STRAY_INVOKE_TOKENS.contains(&line) {
                if line == self.repeat_guard_last_line {
                    self.repeat_guard_run += 1;
                } else {
                    self.repeat_guard_last_line = line.to_string();
                    self.repeat_guard_run = 1;
                }
                if self.repeat_guard_run >= REPEAT_GUARD_TRIP_THRESHOLD {
                    // Trips: discards this line and all subsequent text this round. Already passed kept retain
                    // (a small number of repeats within the threshold is harmless), but no longer appends, and marks tripped.
                    self.repeat_guard_tripped = true;
                    return kept;
                }
                // Within the threshold: passes as usual (a small number of guide word repeats is normal).
                kept.push_str(segment);
            } else {
                // Ordinary text line (including blank lines): resets the repeat count and passes normally.
                if !line.is_empty() {
                    self.repeat_guard_last_line = line.to_string();
                    self.repeat_guard_run = 0;
                }
                kept.push_str(segment);
            }
        }
        kept
    }

    fn emit_text_delta_raw(&mut self, text: &str) -> Vec<SseEvent> {
        let mut events = Vec::new();

        // 🛑 repeat circuit breaker(root cause: Opus long context degradation, put the same stray token repeat line by line infinitely).
        // Filters at the text exit: once the same short line repeats consecutively beyond the threshold, discards the subsequent repeated text,
        // Neither lets it spray to the client nor burns up max_tokens, and also not written into conversation history (breaks the snowball).
        let kept = self.repeat_guard_filter(text);
        if kept.is_empty() {
            return events;
        }
        let text: &str = &kept;

        // 🅱 Maintains the cross stream code fence parity state: everything that is truly emitted as text passes through here,
        // Advances the fence state here so subsequent <invoke> Can determine whether it falls within a code block.
        let mut fence_open = self.code_fence_open;
        let mut fence_partial = std::mem::take(&mut self.fence_scan_partial);
        advance_code_fence_state(&mut fence_open, &mut fence_partial, text);
        self.code_fence_open = fence_open;
        self.fence_scan_partial = fence_partial;

        // if current text_block_index The pointed block has already been closed (for example tool_use startwhenautomatic stop),
        // then discards that index and creates a new text block to continue output, avoiding delta rejected by the state machine causing“swallow characters”.
        if let Some(idx) = self.text_block_index {
            if !self.state_manager.is_block_open_of_type(idx, "text") {
                self.text_block_index = None;
            }
        }

        // get or create the text block index
        let text_index = if let Some(idx) = self.text_block_index {
            idx
        } else {
            // The text block has not been created yet; it must be created first.
            let idx = self.state_manager.next_block_index();
            self.text_block_index = Some(idx);

            // send content_block_start event
            let start_events = self.state_manager.handle_content_block_start(
                idx,
                "text",
                json!({
                    "type": "content_block_start",
                    "index": idx,
                    "content_block": {
                        "type": "text",
                        "text": ""
                    }
                }),
            );
            events.extend(start_events);
            idx
        };

        // send content_block_delta event
        if let Some(delta_event) = self.state_manager.handle_content_block_delta(
            text_index,
            json!({
                "type": "content_block_delta",
                "index": text_index,
                "delta": {
                    "type": "text_delta",
                    "text": text
                }
            }),
        ) {
            events.push(delta_event);
        }

        events
    }

    fn is_thinking_block_open(&self) -> bool {
        self.thinking_block_index
            .is_some_and(|idx| self.state_manager.is_block_open_of_type(idx, "thinking"))
    }

    fn close_open_text_block(&mut self) -> Vec<SseEvent> {
        let Some(idx) = self.text_block_index else {
            return Vec::new();
        };
        if !self.state_manager.is_block_open_of_type(idx, "text") {
            self.text_block_index = None;
            return Vec::new();
        }
        self.text_block_index = None;
        self.state_manager
            .handle_content_block_stop(idx)
            .into_iter()
            .collect()
    }

    fn ensure_thinking_block(&mut self) -> Vec<SseEvent> {
        if self.is_thinking_block_open() {
            return Vec::new();
        }

        let mut events = Vec::new();
        let buffered = std::mem::take(&mut self.thinking_buffer);
        if !buffered.trim().is_empty() {
            events.extend(self.create_text_delta_events(&buffered));
        }
        events.extend(self.close_open_text_block());

        let idx = self.state_manager.next_block_index();
        self.thinking_block_index = Some(idx);
        self.thinking_extracted = true;
        events.extend(self.state_manager.handle_content_block_start(
            idx,
            "thinking",
            json!({
                "type": "content_block_start",
                "index": idx,
                "content_block": {
                    "type": "thinking",
                    "thinking": ""
                }
            }),
        ));
        events
    }

    fn close_open_thinking_block(&mut self) -> Vec<SseEvent> {
        let Some(idx) = self.thinking_block_index else {
            return Vec::new();
        };
        if !self.state_manager.is_block_open_of_type(idx, "thinking") {
            return Vec::new();
        }

        let signature = self
            .pending_thinking_signature
            .take()
            .unwrap_or_else(|| THINKING_SIGNATURE_PLACEHOLDER.to_string());
        let mut events = vec![
            self.create_thinking_delta_event(idx, ""),
            self.create_signature_delta_event_with(idx, &signature),
        ];
        if let Some(stop_event) = self.state_manager.handle_content_block_stop(idx) {
            events.push(stop_event);
        }
        events
    }

    fn process_reasoning_content(
        &mut self,
        reasoning: &crate::kiro::model::events::ReasoningContentEvent,
    ) -> Vec<SseEvent> {
        if !self.thinking_enabled {
            if let Some(text) = reasoning.text.as_deref()
                && !text.is_empty()
            {
                self.output_tokens += estimate_tokens(text);
                return self.create_text_delta_events(text);
            }
            return Vec::new();
        }

        let mut events = Vec::new();

        if let Some(signature) = reasoning.signature.as_deref()
            && !signature.is_empty()
        {
            self.pending_thinking_signature = Some(signature.to_string());
        }

        if let Some(text) = reasoning.text.as_deref()
            && !text.is_empty()
        {
            self.output_tokens += estimate_tokens(text);
            events.extend(self.ensure_thinking_block());
            if let Some(idx) = self.thinking_block_index {
                events.push(self.create_thinking_delta_event(idx, text));
            }
        }

        if let Some(redacted) = reasoning.redacted_content.as_deref()
            && !redacted.is_empty()
        {
            self.output_tokens += 8;
            events.extend(self.create_redacted_thinking_events(redacted));
        }

        events
    }

    fn create_redacted_thinking_events(&mut self, data: &str) -> Vec<SseEvent> {
        let mut events = self.close_open_thinking_block();
        events.extend(self.close_open_text_block());

        let idx = self.state_manager.next_block_index();
        events.extend(self.state_manager.handle_content_block_start(
            idx,
            "redacted_thinking",
            json!({
                "type": "content_block_start",
                "index": idx,
                "content_block": {
                    "type": "redacted_thinking",
                    "data": data
                }
            }),
        ));
        if let Some(stop_event) = self.state_manager.handle_content_block_stop(idx) {
            events.push(stop_event);
        }
        events
    }

    /// create thinking_delta event
    fn create_thinking_delta_event(&self, index: i32, thinking: &str) -> SseEvent {
        SseEvent::new(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": index,
                "delta": {
                    "type": "thinking_delta",
                    "thinking": thinking
                }
            }),
        )
    }

    /// create signature_delta event
    ///
    /// Anthropic under protocol thinking Before the block streaming ends, one must be sent signature_delta,
    /// SDK willit aggregatesto thinking block `signature` field. On the next round the client
    /// assistant local validation when the message is returned thinking blockmust carrynonempty signature, otherwisethrow
    /// `The content[].thinking in the thinking mode must be passed back to the API`.
    ///
    /// upstream Kiro is not Anthropic The server does not deliver a real signature, so here it sends a non empty
    /// Placeholder string to satisfy client local validation. This field is not forwarded back. Kiro logic
    /// (converter read only `block.thinking`, do not read signature).
    fn create_signature_delta_event(&self, index: i32) -> SseEvent {
        self.create_signature_delta_event_with(index, THINKING_SIGNATURE_PLACEHOLDER)
    }

    fn create_signature_delta_event_with(&self, index: i32, signature: &str) -> SseEvent {
        SseEvent::new(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": index,
                "delta": {
                    "type": "signature_delta",
                    "signature": signature,
                }
            }),
        )
    }

    /// handle the tool use event
    fn process_tool_use(
        &mut self,
        tool_use: &crate::kiro::model::events::ToolUseEvent,
    ) -> Vec<SseEvent> {
        let mut events = Vec::new();

        self.state_manager.set_has_tool_use(true);

        if self.is_thinking_block_open() && !self.in_thinking_block {
            events.extend(self.close_open_thinking_block());
        }

        // tool_use must occurin thinking endafter.
        // but when `</thinking>` nothing after `\n\n`(for exampleimmediately follow tool_use or when the stream ends,
        // thinking the closing tag will linger in thinking_buffer, causingsubsequent flush take when `</thinking>` output as content.
        // hereinstart tool_use block do once before“boundary case”end tag recognition and filtering.
        if self.thinking_enabled && self.in_thinking_block {
            if let Some(end_pos) = find_real_thinking_end_tag_at_buffer_end(&self.thinking_buffer) {
                let thinking_content = self.thinking_buffer[..end_pos].to_string();
                if !thinking_content.is_empty() {
                    if let Some(thinking_index) = self.thinking_block_index {
                        events.push(
                            self.create_thinking_delta_event(thinking_index, &thinking_content),
                        );
                    }
                }

                // end thinking block
                self.in_thinking_block = false;
                self.thinking_extracted = true;

                if let Some(thinking_index) = self.thinking_block_index {
                    // firstsend empty thinking_delta
                    events.push(self.create_thinking_delta_event(thinking_index, ""));
                    // signature_delta:satisfyclient thinking local validation under the mode
                    events.push(self.create_signature_delta_event(thinking_index));
                    // then send content_block_stop
                    if let Some(stop_event) =
                        self.state_manager.handle_content_block_stop(thinking_index)
                    {
                        events.push(stop_event);
                    }
                }

                // Treats content after the end tag as plain text (usually empty or whitespace).
                let after_pos = end_pos + "</thinking>".len();
                let remaining = self.thinking_buffer[after_pos..].trim_start().to_string();
                self.thinking_buffer.clear();
                if !remaining.is_empty() {
                    events.extend(self.create_text_delta_events(&remaining));
                }
            }
        }

        // thinking under the mode,process_content_with_thinking may for the sake of probing `<thinking>` and temporarily holds a short piece of tail text.
        // if we start directly at this point tool_use, the state machine will close automatically text block, causingthissegment"pendingoutputtext"appears to be tool_use swallow.
        // constraint: only when not yet entered thinking block, and thinking When not yet extracted, treats the buffer as plain text. flush.
        if self.thinking_enabled
            && !self.in_thinking_block
            && !self.thinking_extracted
            && !self.thinking_buffer.is_empty()
        {
            let buffered = std::mem::take(&mut self.thinking_buffer);
            events.extend(self.create_text_delta_events(&buffered));
        }

        // get or allocate the block index
        let block_index = if let Some(&idx) = self.tool_block_indices.get(&tool_use.tool_use_id) {
            idx
        } else {
            let idx = self.state_manager.next_block_index();
            self.tool_block_indices
                .insert(tool_use.tool_use_id.clone(), idx);
            idx
        };

        // Restores the tool name (if there is a mapping).
        let original_name = self
            .tool_name_map
            .get(&tool_use.name)
            .cloned()
            .unwrap_or_else(|| tool_use.name.clone());

        // send content_block_start
        let start_events = self.state_manager.handle_content_block_start(
            block_index,
            "tool_use",
            json!({
                "type": "content_block_start",
                "index": block_index,
                "content_block": {
                    "type": "tool_use",
                    "id": tool_use.tool_use_id,
                    "name": original_name,
                    "input": {}
                }
            }),
        );
        events.extend(start_events);

        // sendparameterincrement (ToolUseEvent.input is String type)
        if !tool_use.input.is_empty() {
            self.output_tokens += (tool_use.input.len() as i32 + 3) / 4; // estimate token

            if let Some(delta_event) = self.state_manager.handle_content_block_delta(
                block_index,
                json!({
                    "type": "content_block_delta",
                    "index": block_index,
                    "delta": {
                        "type": "input_json_delta",
                        "partial_json": tool_use.input
                    }
                }),
            ) {
                events.push(delta_event);
            }
        }

        // If it is a complete tool call (stop=true), send content_block_stop
        if tool_use.stop {
            if let Some(stop_event) = self.state_manager.handle_content_block_stop(block_index) {
                events.push(stop_event);
            }
        }

        events
    }

    /// generate the final event sequence
    pub fn generate_final_events(&mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();

        if self.is_thinking_block_open() && !self.in_thinking_block {
            events.extend(self.close_open_thinking_block());
        }

        // Flush thinking_buffer inremainingcontent
        if self.thinking_enabled && !self.thinking_buffer.is_empty() {
            if self.in_thinking_block {
                // the end cancanleftover `</thinking>`(for exampleimmediately follow tool_use or the stream ends), needs to at flush filter out the closing tag.
                if let Some(end_pos) =
                    find_real_thinking_end_tag_at_buffer_end(&self.thinking_buffer)
                {
                    let thinking_content = self.thinking_buffer[..end_pos].to_string();
                    if !thinking_content.is_empty() {
                        if let Some(thinking_index) = self.thinking_block_index {
                            events.push(
                                self.create_thinking_delta_event(thinking_index, &thinking_content),
                            );
                        }
                    }

                    // close thinking block: first send an empty thinking_delta, then send content_block_stop
                    if let Some(thinking_index) = self.thinking_block_index {
                        events.push(self.create_thinking_delta_event(thinking_index, ""));
                        // signature_delta:satisfyclient thinking local validation under the mode
                        events.push(self.create_signature_delta_event(thinking_index));
                        if let Some(stop_event) =
                            self.state_manager.handle_content_block_stop(thinking_index)
                        {
                            events.push(stop_event);
                        }
                    }

                    // Treats content after the end tag as plain text (usually empty or whitespace).
                    let after_pos = end_pos + "</thinking>".len();
                    let remaining = self.thinking_buffer[after_pos..].trim_start().to_string();
                    self.thinking_buffer.clear();
                    self.in_thinking_block = false;
                    self.thinking_extracted = true;
                    if !remaining.is_empty() {
                        events.extend(self.create_text_delta_events(&remaining));
                    }
                } else {
                    // if still in thinking inside the block, sends the remaining content as thinking_delta
                    if let Some(thinking_index) = self.thinking_block_index {
                        events.push(
                            self.create_thinking_delta_event(thinking_index, &self.thinking_buffer),
                        );
                    }
                    // close thinking block: first send an empty thinking_delta, then send content_block_stop
                    if let Some(thinking_index) = self.thinking_block_index {
                        // firstsend empty thinking_delta
                        events.push(self.create_thinking_delta_event(thinking_index, ""));
                        // signature_delta:satisfyclient thinking local validation under the mode
                        events.push(self.create_signature_delta_event(thinking_index));
                        // then send content_block_stop
                        if let Some(stop_event) =
                            self.state_manager.handle_content_block_stop(thinking_index)
                        {
                            events.push(stop_event);
                        }
                    }
                }
            } else {
                // otherwise send the remaining content as text_delta
                let buffer_content = self.thinking_buffer.clone();
                events.extend(self.create_text_delta_events(&buffer_content));
            }
            self.thinking_buffer.clear();
        }

        // if the whole stream only produced thinking block, none text also none tool_use,
        // then set stop_reason as max_tokens(means the model exhausted token the budget on thinking),
        // and resend a complete set of text event (content is a single space), ensuring content array has text block
        if self.thinking_enabled
            && self.thinking_block_index.is_some()
            && !self.state_manager.has_non_thinking_blocks()
        {
            self.state_manager.set_stop_reason("max_tokens");
            events.extend(self.create_text_delta_events(" "));
        }

        // Flush invoke Sniff buffer remainder: sniff once more for a complete block first (in case the last piece is complete) invoke),
        // remaining go emit_text_delta_raw flush emit out (to prevent trailing byte loss).
        if !self.invoke_sniff_buffer.is_empty() {
            events.extend(self.drain_invoke_sniff_buffer(true));
        }

        // mutually exclusivebasis:total truthy (contextUsage priority)− cache override = uncached input.
        let (final_input_tokens, cache_creation, cache_read) = self.resolved_usage();

        // generatefinalevent
        events.extend(self.state_manager.generate_final_events(
            final_input_tokens,
            self.output_tokens,
            cache_creation,
            cache_read,
        ));
        events
    }
}

/// buffered stream processing context - used for /cc/v1/messages streaming request
///
/// and `StreamContext` Different, this context buffers all events until the stream ends,
/// soafterusefrom `contextUsageEvent` computeofcorrect `input_tokens` correct `message_start` event.
///
/// workflow:
/// 1. use `StreamContext` normalhandleall Kiro event
/// 2. take generated SSE Caches the event (rather than sending immediately).
/// 3. when the stream ends, find `message_start` eventandupdateits `input_tokens`
/// 4. return all events at once
pub struct BufferedStreamContext {
    /// Internal stream processing context (reuses the existing event handling logic).
    inner: StreamContext,
    /// all buffered events (including message_start,content_block_start etc.)
    event_buffer: Vec<SseEvent>,
    /// Whether the initial event has already been generated.
    initial_events_generated: bool,
}

impl BufferedStreamContext {
    /// create a buffered stream context
    pub fn new(
        model: impl Into<String>,
        estimated_input_tokens: i32,
        thinking_enabled: bool,
        tool_name_map: HashMap<String, String>,
        known_tool_names: std::collections::HashSet<String>,
    ) -> Self {
        let inner = StreamContext::new_with_thinking(
            model,
            estimated_input_tokens,
            thinking_enabled,
            tool_name_map,
            known_tool_names,
        );
        Self {
            inner,
            event_buffer: Vec::new(),
            initial_events_generated: false,
        }
    }

    /// injected by CacheMeter computed cache coverage (estimate basis), apportioned at final report.
    pub fn set_cache_usage(&mut self, cache_usage: super::cache_metering::CacheUsage) {
        self.inner.cache_usage = cache_usage;
    }

    /// handle Kiro the event and buffer the result
    ///
    /// reuse StreamContext event handling logic, but caches the result instead of sending immediately.
    pub fn process_and_buffer(&mut self, event: &crate::kiro::model::events::Event) {
        // When processing an event for the first time, first generates the initial event (message_start etc.)
        if !self.initial_events_generated {
            let initial_events = self.inner.generate_initial_events();
            self.event_buffer.extend(initial_events);
            self.initial_events_generated = true;
        }

        // process the event and buffer the result
        let events = self.inner.process_kiro_event(event);
        self.event_buffer.extend(events);
    }

    /// Completes stream processing and returns all events.
    ///
    /// thismethodwill:
    /// 1. generate the final event (message_delta, message_stop)
    /// 2. with correct input_tokens correct message_start event
    /// 3. return all buffered events
    pub fn finish_and_get_all_events(&mut self) -> Vec<SseEvent> {
        // If no event was ever processed, still generates the initial event.
        if !self.initial_events_generated {
            let initial_events = self.inner.generate_initial_events();
            self.event_buffer.extend(initial_events);
            self.initial_events_generated = true;
        }

        // mutually exclusive measure apportionment:total truthy − cache override = uncached input(with inner wrap upconsistent).
        let (final_input_tokens, cache_creation, cache_read) = self.inner.resolved_usage();

        // generate the final event (StreamContext internally uses the same priority and apportionment)
        let final_events = self.inner.generate_final_events();
        self.event_buffer.extend(final_events);

        // correct message_start in event input_tokens and cache_* field
        for event in &mut self.event_buffer {
            if event.event == "message_start" {
                if let Some(message) = event.data.get_mut("message") {
                    if let Some(usage) = message.get_mut("usage") {
                        usage["input_tokens"] = serde_json::json!(final_input_tokens);
                        usage["cache_creation_input_tokens"] = serde_json::json!(cache_creation);
                        usage["cache_read_input_tokens"] = serde_json::json!(cache_read);
                    }
                }
            }
        }

        std::mem::take(&mut self.event_buffer)
    }

    /// take out the final usage (at finish_and_get_all_events aftercall)
    ///
    /// returnorder:(input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens, credits)
    pub fn final_usage(&self) -> (i32, i32, i32, i32, f64) {
        let (input, creation, read) = self.inner.resolved_usage();
        (
            input,
            self.inner.output_tokens,
            creation,
            read,
            self.inner.credits,
        )
    }
}

/// simple token Estimate (mixed Chinese and English characters).
///
/// public for cache_meter and similar modules reuse the same estimation basis.
pub fn estimate_tokens(text: &str) -> i32 {
    let chars: Vec<char> = text.chars().collect();
    let mut chinese_count = 0;
    let mut other_count = 0;

    for c in &chars {
        if *c >= '\u{4E00}' && *c <= '\u{9FFF}' {
            chinese_count += 1;
        } else {
            other_count += 1;
        }
    }

    // Chinese about 1.5 character/token, English about 4 character/token
    let chinese_tokens = (chinese_count * 2 + 2) / 3;
    let other_tokens = (other_count + 3) / 4;

    (chinese_tokens + other_tokens).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The known tool table for testing: contains invoke The tool name synthesized in tests,
    /// let 🅳 Tool table validation passes these names, so the recovery logic itself can be verified.
    fn test_known_tools() -> std::collections::HashSet<String> {
        ["exec_command", "apply_patch", "tool_a", "tool_b", "write_file", "wait_agent"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    // ---- extract_invoke_content_blocks: one-shot (non-streaming) reclamation ----

    #[test]
    fn extract_blocks_reclaims_clean_leak_and_strips_stray_token() {
        let text = "call\n<invoke name=\"exec_command\">\n<parameter name=\"cmd\">echo hi</parameter>\n</invoke>";
        let blocks = extract_invoke_content_blocks(
            text,
            &test_known_tools(),
            &std::collections::HashMap::new(),
        );
        let tu = blocks
            .iter()
            .find(|b| b["type"] == "tool_use")
            .expect("must reclaim tool_use");
        assert_eq!(tu["name"], "exec_command");
        assert_eq!(tu["input"]["cmd"], "echo hi");
        assert!(
            !blocks.iter().any(|b| b["type"] == "text"
                && b["text"].as_str().map(|t| t.contains("<invoke")).unwrap_or(false)),
            "no literal <invoke> may remain as text"
        );
        assert!(
            !blocks.iter().any(|b| b["type"] == "text" && b["text"] == "call\n"),
            "stray token line must be stripped"
        );
    }

    #[test]
    fn extract_blocks_restores_shortened_name_via_map() {
        let short = "shrunk_name_abcd1234";
        let original = "an_extremely_long_original_tool_name_that_exceeds_the_limit";
        let text = format!(
            "call\n<invoke name=\"{}\">\n<parameter name=\"x\">y</parameter>\n</invoke>",
            short
        );
        let mut known = std::collections::HashSet::new();
        known.insert(short.to_string());
        let mut map = std::collections::HashMap::new();
        map.insert(short.to_string(), original.to_string());
        let blocks = extract_invoke_content_blocks(&text, &known, &map);
        let tu = blocks.iter().find(|b| b["type"] == "tool_use").expect("reclaimed");
        assert_eq!(tu["name"], original, "shortened name must be restored to original");
    }

    #[test]
    fn extract_blocks_does_not_reclaim_fenced_or_unknown() {
        // fenced -> display, not reclaimed
        let fenced = "see:\n```\n<invoke name=\"exec_command\">\n<parameter name=\"cmd\">rm -rf /</parameter>\n</invoke>\n```";
        let b1 = extract_invoke_content_blocks(fenced, &test_known_tools(), &std::collections::HashMap::new());
        assert!(!b1.iter().any(|b| b["type"] == "tool_use"), "fenced must not reclaim");
        // unknown tool name -> not reclaimed
        let unknown = "call\n<invoke name=\"not_a_real_tool\">\n<parameter name=\"x\">y</parameter>\n</invoke>";
        let b2 = extract_invoke_content_blocks(unknown, &test_known_tools(), &std::collections::HashMap::new());
        assert!(!b2.iter().any(|b| b["type"] == "tool_use"), "unknown name must not reclaim");
    }

    #[test]
    fn extract_blocks_clean_text_is_single_unchanged_text_block() {
        let blocks = extract_invoke_content_blocks(
            "just a normal answer with no tool calls",
            &test_known_tools(),
            &std::collections::HashMap::new(),
        );
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[0]["text"], "just a normal answer with no tool calls");
    }

    #[test]
    fn test_sse_event_format() {
        let event = SseEvent::new("message_start", json!({"type": "message_start"}));
        let sse_str = event.to_sse_string();

        assert!(sse_str.starts_with("event: message_start\n"));
        assert!(sse_str.contains("data: "));
        assert!(sse_str.ends_with("\n\n"));
    }

    #[test]
    fn test_sse_state_manager_message_start() {
        let mut manager = SseStateManager::new();

        // the first time should succeed
        let event = manager.handle_message_start(json!({"type": "message_start"}));
        assert!(event.is_some());

        // the second time should be skipped
        let event = manager.handle_message_start(json!({"type": "message_start"}));
        assert!(event.is_none());
    }

    #[test]
    fn test_sse_state_manager_block_lifecycle() {
        let mut manager = SseStateManager::new();

        // create block
        let events = manager.handle_content_block_start(0, "text", json!({}));
        assert_eq!(events.len(), 1);

        // delta
        let event = manager.handle_content_block_delta(0, json!({}));
        assert!(event.is_some());

        // stop
        let event = manager.handle_content_block_stop(0);
        assert!(event.is_some());

        // duplicate stop shouldthisskipped
        let event = manager.handle_content_block_stop(0);
        assert!(event.is_none());
    }

    #[test]
    fn test_tool_name_reverse_mapping_in_stream() {
        use crate::kiro::model::events::ToolUseEvent;

        let mut map = HashMap::new();
        map.insert(
            "short_abc12345".to_string(),
            "mcp__very_long_original_tool_name".to_string(),
        );

        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, map, test_known_tools());
        let _ = ctx.generate_initial_events();

        // simulate Kiro returnshort nameof tool_use
        let tool_event = Event::ToolUse(ToolUseEvent {
            name: "short_abc12345".to_string(),
            tool_use_id: "toolu_01".to_string(),
            input: r#"{"key":"value"}"#.to_string(),
            stop: true,
        });

        let events = ctx.process_kiro_event(&tool_event);

        // content_block_start in name should be the original long name
        let start_event = events
            .iter()
            .find(|e| e.event == "content_block_start")
            .unwrap();
        assert_eq!(
            start_event.data["content_block"]["name"], "mcp__very_long_original_tool_name",
            "should be restored to the original tool name"
        );
    }

    #[test]
    fn test_text_delta_after_tool_use_restarts_text_block() {
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());

        let initial_events = ctx.generate_initial_events();
        assert!(
            initial_events
                .iter()
                .any(|e| e.event == "content_block_start"
                    && e.data["content_block"]["type"] == "text")
        );

        let initial_text_index = ctx
            .text_block_index
            .expect("initial text block index should exist");

        // tool_use starting will automatically close the existing text block
        let tool_events = ctx.process_tool_use(&crate::kiro::model::events::ToolUseEvent {
            name: "test_tool".to_string(),
            tool_use_id: "tool_1".to_string(),
            input: "{}".to_string(),
            stop: false,
        });
        assert!(
            tool_events.iter().any(|e| {
                e.event == "content_block_stop"
                    && e.data["index"].as_i64() == Some(initial_text_index as i64)
            }),
            "tool_use should stop the previous text block"
        );

        // when a text delta arrives afterward, should automatically create a new text block andis nottowardalready stop write in block delta
        let text_events = ctx.process_assistant_response("hello");
        let new_text_start_index = text_events.iter().find_map(|e| {
            if e.event == "content_block_start" && e.data["content_block"]["type"] == "text" {
                e.data["index"].as_i64()
            } else {
                None
            }
        });
        assert!(
            new_text_start_index.is_some(),
            "should start a new text block"
        );
        assert_ne!(
            new_text_start_index.unwrap(),
            initial_text_index as i64,
            "new text block index should differ from the stopped one"
        );
        assert!(
            text_events.iter().any(|e| {
                e.event == "content_block_delta"
                    && e.data["delta"]["type"] == "text_delta"
                    && e.data["delta"]["text"] == "hello"
            }),
            "should emit text_delta after restarting text block"
        );
    }

    #[test]
    fn test_tool_use_flushes_pending_thinking_buffer_text_before_tool_block() {
        // thinking In this mode, short text may be temporarily held in thinking_buffer to wait for `<thinking>` cross chunk match.
        // whenappears immediately after tool_use when, should first flush this text, then start tool_use block.
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _initial_events = ctx.generate_initial_events();

        // two segments of short text (each 2 Chinese characters); the total length may still be insufficient to satisfy safe_len>0 ofoutputentryitem,
        // and therefore will leavein thinking_buffer inetc.pendingsubsequent chunk.
        let ev1 = ctx.process_assistant_response("has mod");
        assert!(
            ev1.iter().all(|e| e.event != "content_block_delta"),
            "short prefix should be buffered under thinking mode"
        );
        let ev2 = ctx.process_assistant_response("edit:");
        assert!(
            ev2.iter().all(|e| e.event != "content_block_delta"),
            "short prefix should still be buffered under thinking mode"
        );

        let events = ctx.process_tool_use(&crate::kiro::model::events::ToolUseEvent {
            name: "Write".to_string(),
            tool_use_id: "tool_1".to_string(),
            input: "{}".to_string(),
            stop: false,
        });

        let text_start_index = events.iter().find_map(|e| {
            if e.event == "content_block_start" && e.data["content_block"]["type"] == "text" {
                e.data["index"].as_i64()
            } else {
                None
            }
        });
        let pos_text_delta = events.iter().position(|e| {
            e.event == "content_block_delta" && e.data["delta"]["type"] == "text_delta"
        });
        let pos_text_stop = text_start_index.and_then(|idx| {
            events.iter().position(|e| {
                e.event == "content_block_stop" && e.data["index"].as_i64() == Some(idx)
            })
        });
        let pos_tool_start = events.iter().position(|e| {
            e.event == "content_block_start" && e.data["content_block"]["type"] == "tool_use"
        });

        assert!(
            text_start_index.is_some(),
            "should start a text block to flush buffered text"
        );
        assert!(
            pos_text_delta.is_some(),
            "should flush buffered text as text_delta"
        );
        assert!(
            pos_text_stop.is_some(),
            "should stop text block before tool_use block starts"
        );
        assert!(pos_tool_start.is_some(), "should start tool_use block");

        let pos_text_delta = pos_text_delta.unwrap();
        let pos_text_stop = pos_text_stop.unwrap();
        let pos_tool_start = pos_tool_start.unwrap();

        assert!(
            pos_text_delta < pos_text_stop && pos_text_stop < pos_tool_start,
            "ordering should be: text_delta -> text_stop -> tool_use_start"
        );

        assert!(
            events.iter().any(|e| {
                e.event == "content_block_delta"
                    && e.data["delta"]["type"] == "text_delta"
                    && e.data["delta"]["text"] == "modified:"
            }),
            "flushed text should equal the buffered prefix"
        );
    }

    #[test]
    fn test_estimate_tokens() {
        assert!(estimate_tokens("Hello") > 0);
        assert!(estimate_tokens("hello") > 0);
        assert!(estimate_tokens("Hello hello") > 0);
    }

    #[test]
    fn test_find_real_thinking_start_tag_basic() {
        // Base case: a normal start tag.
        assert_eq!(find_real_thinking_start_tag("<thinking>"), Some(0));
        assert_eq!(find_real_thinking_start_tag("prefix<thinking>"), Some(6));
    }

    #[test]
    fn test_find_real_thinking_start_tag_with_backticks() {
        // Those wrapped by backticks should be skipped.
        assert_eq!(find_real_thinking_start_tag("`<thinking>`"), None);
        assert_eq!(find_real_thinking_start_tag("use `<thinking>` tag"), None);

        // First a wrapped one, then a real start tag.
        assert_eq!(
            find_real_thinking_start_tag("about `<thinking>` tag<thinking>content"),
            Some(22)
        );
    }

    #[test]
    fn test_find_real_thinking_start_tag_with_quotes() {
        // Those wrapped by double quotes should be skipped.
        assert_eq!(find_real_thinking_start_tag("\"<thinking>\""), None);
        assert_eq!(find_real_thinking_start_tag("the \"<thinking>\" tag"), None);

        // Those wrapped by single quotes should be skipped.
        assert_eq!(find_real_thinking_start_tag("'<thinking>'"), None);

        // mixed case
        assert_eq!(
            find_real_thinking_start_tag("about \"<thinking>\" and '<thinking>' then<thinking>"),
            Some(40)
        );
    }

    #[test]
    fn test_find_real_thinking_end_tag_basic() {
        // Base case: a normal end tag is followed by a double newline.
        assert_eq!(find_real_thinking_end_tag("</thinking>\n\n"), Some(0));
        assert_eq!(
            find_real_thinking_end_tag("content</thinking>\n\n"),
            Some(7)
        );
        assert_eq!(
            find_real_thinking_end_tag("some text</thinking>\n\nmore text"),
            Some(9)
        );

        // the case without a double newline
        assert_eq!(find_real_thinking_end_tag("</thinking>"), None);
        assert_eq!(find_real_thinking_end_tag("</thinking>\n"), None);
        assert_eq!(find_real_thinking_end_tag("</thinking> more"), None);
    }

    #[test]
    fn test_find_real_thinking_end_tag_with_backticks() {
        // Those wrapped by backticks should be skipped.
        assert_eq!(find_real_thinking_end_tag("`</thinking>`\n\n"), None);
        assert_eq!(
            find_real_thinking_end_tag("mention `</thinking>` in code\n\n"),
            None
        );

        // only a backtick in front
        assert_eq!(find_real_thinking_end_tag("`</thinking>\n\n"), None);

        // only a backtick behind
        assert_eq!(find_real_thinking_end_tag("</thinking>`\n\n"), None);
    }

    #[test]
    fn test_find_real_thinking_end_tag_with_quotes() {
        // Those wrapped by double quotes should be skipped.
        assert_eq!(find_real_thinking_end_tag("\"</thinking>\"\n\n"), None);
        assert_eq!(
            find_real_thinking_end_tag("the string \"</thinking>\" is a tag\n\n"),
            None
        );

        // Those wrapped by single quotes should be skipped.
        assert_eq!(find_real_thinking_end_tag("'</thinking>'\n\n"), None);
        assert_eq!(
            find_real_thinking_end_tag("use '</thinking>' as marker\n\n"),
            None
        );

        // Mixed case: after a double quote wrap there is a real tag.
        assert_eq!(
            find_real_thinking_end_tag("about \"</thinking>\" tag</thinking>\n\n"),
            Some(23)
        );

        // Mixed case: after a single quote wrap there is a real tag.
        assert_eq!(
            find_real_thinking_end_tag("about '</thinking>' tag</thinking>\n\n"),
            Some(23)
        );
    }

    #[test]
    fn test_find_real_thinking_end_tag_mixed() {
        // First a wrapped one, then a real end tag.
        assert_eq!(
            find_real_thinking_end_tag("discussing `</thinking>` tag</thinking>\n\n"),
            Some(28)
        );

        // Multiple wrapped ones, the last is the real one.
        assert_eq!(
            find_real_thinking_end_tag("`</thinking>` and `</thinking>` done</thinking>\n\n"),
            Some(36)
        );

        // mixed multiple quote characters
        assert_eq!(
            find_real_thinking_end_tag(
                "`</thinking>` and \"</thinking>\" and '</thinking>' done</thinking>\n\n"
            ),
            Some(54)
        );
    }

    #[test]
    fn test_tool_use_immediately_after_thinking_filters_end_tag_and_closes_thinking_block() {
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _initial_events = ctx.generate_initial_events();

        let mut all_events = Vec::new();

        // thinking content starts with `</thinking>` at the end, but nothing behind `\n\n`(simulateimmediately follow tool_use scenario)
        all_events.extend(ctx.process_assistant_response("<thinking>abc</thinking>"));

        let tool_events = ctx.process_tool_use(&crate::kiro::model::events::ToolUseEvent {
            name: "Write".to_string(),
            tool_use_id: "tool_1".to_string(),
            input: "{}".to_string(),
            stop: false,
        });
        all_events.extend(tool_events);

        all_events.extend(ctx.generate_final_events());

        // should not `</thinking>` treat as thinking content output
        assert!(
            all_events.iter().all(|e| {
                !(e.event == "content_block_delta"
                    && e.data["delta"]["type"] == "thinking_delta"
                    && e.data["delta"]["thinking"] == "</thinking>")
            }),
            "`</thinking>` should be filtered from output"
        );

        // thinking block must be in tool_use block close before
        let thinking_index = ctx
            .thinking_block_index
            .expect("thinking block index should exist");
        let pos_thinking_stop = all_events.iter().position(|e| {
            e.event == "content_block_stop"
                && e.data["index"].as_i64() == Some(thinking_index as i64)
        });
        let pos_tool_start = all_events.iter().position(|e| {
            e.event == "content_block_start" && e.data["content_block"]["type"] == "tool_use"
        });
        assert!(
            pos_thinking_stop.is_some(),
            "thinking block should be stopped"
        );
        assert!(pos_tool_start.is_some(), "tool_use block should be started");
        assert!(
            pos_thinking_stop.unwrap() < pos_tool_start.unwrap(),
            "thinking block should stop before tool_use block starts"
        );
    }

    #[test]
    fn test_thinking_block_emits_signature_delta_before_stop() {
        // client at thinking under the moderequire thinking block carries signature field; otherwise on the next round when returned
        // will throw "must be passed back to the API".thistestverify thinking sent before the block ends
        // anonemptyof signature_delta event.
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response("<thinking>abc</thinking>\n\nhello"));
        all.extend(ctx.generate_final_events());

        let thinking_index = ctx
            .thinking_block_index
            .expect("thinking block index should exist");

        let pos_sig = all.iter().position(|e| {
            e.event == "content_block_delta"
                && e.data["index"].as_i64() == Some(thinking_index as i64)
                && e.data["delta"]["type"] == "signature_delta"
                && e.data["delta"]["signature"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty())
        });
        let pos_stop = all.iter().position(|e| {
            e.event == "content_block_stop"
                && e.data["index"].as_i64() == Some(thinking_index as i64)
        });

        assert!(pos_sig.is_some(), "signature_delta should be emitted");
        assert!(pos_stop.is_some(), "content_block_stop should be emitted");
        assert!(
            pos_sig.unwrap() < pos_stop.unwrap(),
            "signature_delta must precede content_block_stop"
        );
    }

    #[test]
    fn test_final_flush_filters_standalone_thinking_end_tag() {
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _initial_events = ctx.generate_initial_events();

        let mut all_events = Vec::new();
        all_events.extend(ctx.process_assistant_response("<thinking>abc</thinking>"));
        all_events.extend(ctx.generate_final_events());

        assert!(
            all_events.iter().all(|e| {
                !(e.event == "content_block_delta"
                    && e.data["delta"]["type"] == "thinking_delta"
                    && e.data["delta"]["thinking"] == "</thinking>")
            }),
            "`</thinking>` should be filtered during final flush"
        );
    }

    #[test]
    fn test_thinking_strips_leading_newline_same_chunk() {
        // <thinking>\n in the same chunk in,\n should be stripped
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _initial_events = ctx.generate_initial_events();

        let events = ctx.process_assistant_response("<thinking>\nHello world");

        // find all thinking_delta event
        let thinking_deltas: Vec<_> = events
            .iter()
            .filter(|e| {
                e.event == "content_block_delta" && e.data["delta"]["type"] == "thinking_delta"
            })
            .collect();

        // concatenate all thinking content
        let full_thinking: String = thinking_deltas
            .iter()
            .map(|e| e.data["delta"]["thinking"].as_str().unwrap_or(""))
            .collect();

        assert!(
            !full_thinking.starts_with('\n'),
            "thinking content should not start with \\n, got: {:?}",
            full_thinking
        );
    }

    #[test]
    fn test_thinking_strips_leading_newline_cross_chunk() {
        // <thinking> in the first chunk at end,\n in the second chunk start
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _initial_events = ctx.generate_initial_events();

        let events1 = ctx.process_assistant_response("<thinking>");
        let events2 = ctx.process_assistant_response("\nHello world");

        let mut all_events = Vec::new();
        all_events.extend(events1);
        all_events.extend(events2);

        let thinking_deltas: Vec<_> = all_events
            .iter()
            .filter(|e| {
                e.event == "content_block_delta" && e.data["delta"]["type"] == "thinking_delta"
            })
            .collect();

        let full_thinking: String = thinking_deltas
            .iter()
            .map(|e| e.data["delta"]["thinking"].as_str().unwrap_or(""))
            .collect();

        assert!(
            !full_thinking.starts_with('\n'),
            "thinking content should not start with \\n across chunks, got: {:?}",
            full_thinking
        );
    }

    #[test]
    fn test_thinking_no_strip_when_no_leading_newline() {
        // <thinking> directly followed by content (no \n), the content should be fully preserved
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _initial_events = ctx.generate_initial_events();

        let events = ctx.process_assistant_response("<thinking>abc</thinking>\n\ntext");

        let thinking_deltas: Vec<_> = events
            .iter()
            .filter(|e| {
                e.event == "content_block_delta" && e.data["delta"]["type"] == "thinking_delta"
            })
            .collect();

        let full_thinking: String = thinking_deltas
            .iter()
            .filter(|e| {
                !e.data["delta"]["thinking"]
                    .as_str()
                    .unwrap_or("")
                    .is_empty()
            })
            .map(|e| e.data["delta"]["thinking"].as_str().unwrap_or(""))
            .collect();

        assert_eq!(full_thinking, "abc", "thinking content should be 'abc'");
    }

    #[test]
    fn test_text_after_thinking_strips_leading_newlines() {
        // `</thinking>\n\n` the text after should not \n\n start
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _initial_events = ctx.generate_initial_events();

        let events = ctx.process_assistant_response("<thinking>\nabc</thinking>\n\nhello");

        let text_deltas: Vec<_> = events
            .iter()
            .filter(|e| e.event == "content_block_delta" && e.data["delta"]["type"] == "text_delta")
            .collect();

        let full_text: String = text_deltas
            .iter()
            .map(|e| e.data["delta"]["text"].as_str().unwrap_or(""))
            .collect();

        assert!(
            !full_text.starts_with('\n'),
            "text after thinking should not start with \\n, got: {:?}",
            full_text
        );
        assert_eq!(full_text, "hello");
    }

    /// Helper function: extracts all from the event list. thinking_delta ofconcatenatecontent
    fn collect_thinking_content(events: &[SseEvent]) -> String {
        events
            .iter()
            .filter(|e| {
                e.event == "content_block_delta" && e.data["delta"]["type"] == "thinking_delta"
            })
            .map(|e| e.data["delta"]["thinking"].as_str().unwrap_or(""))
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Helper function: extracts all from the event list. text_delta ofconcatenatecontent
    fn collect_text_content(events: &[SseEvent]) -> String {
        events
            .iter()
            .filter(|e| e.event == "content_block_delta" && e.data["delta"]["type"] == "text_delta")
            .map(|e| e.data["delta"]["text"].as_str().unwrap_or(""))
            .collect()
    }

    /// Helper function: extracts all synthesized ones from the event list. tool_use call
    ///
    /// capture `content_block_start` in `content_block.type == "tool_use"` of name,
    /// then pair the same index of `input_json_delta.partial_json`, return (name, input_json).
    fn collect_tool_uses(events: &[SseEvent]) -> Vec<(String, String)> {
        let mut result = Vec::new();
        for e in events.iter() {
            if e.event == "content_block_start" && e.data["content_block"]["type"] == "tool_use" {
                let index = e.data["index"].as_i64();
                let name = e.data["content_block"]["name"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                // find same index of input_json_delta
                let input = events
                    .iter()
                    .find(|d| {
                        d.event == "content_block_delta"
                            && d.data["index"].as_i64() == index
                            && d.data["delta"]["type"] == "input_json_delta"
                    })
                    .and_then(|d| d.data["delta"]["partial_json"].as_str())
                    .unwrap_or("")
                    .to_string();
                result.push((name, input));
            }
        }
        result
    }

    #[test]
    fn test_invoke_sniff_backtick_wrapped_is_not_captured() {
        // 🔴 Prevents wrongful harm: those wrapped by backticks. <invoke> is a quote, should not be captured
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response("example:`<invoke name=\"x\">` this form"));
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert!(tools.is_empty(), "Those wrapped by backticks should not be caught.: {:?}", tools);

        let text = collect_text_content(&all);
        assert!(
            text.contains("<invoke name=\"x\">"),
            "the original text should be kept as is in text in: {:?}",
            text
        );
    }

    #[test]
    fn test_invoke_sniff_single_bare_invoke() {
        // 🟢 single bare invoke(noshell)
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(
            "<invoke name=\"exec_command\"><parameter name=\"cmd\">ls</parameter></invoke>",
        ));
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert_eq!(tools.len(), 1, "should synthesize 1 item tool_use: {:?}", tools);
        assert_eq!(tools[0].0, "exec_command", "name should be exec_command");
        let parsed: serde_json::Value =
            serde_json::from_str(&tools[0].1).expect("input should be valid JSON");
        assert_eq!(parsed["cmd"], "ls", "input should contain cmd=ls");
    }

    #[test]
    fn test_invoke_sniff_param_value_with_lt_multiline_chinese() {
        // 🟢 parameter value contains `<`,multi line,Chinese → not truncated
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        let value = "first line a < b\nsecond line path /tmp/Chinese";
        let chunk = format!(
            "<invoke name=\"write_file\"><parameter name=\"content\">{}</parameter></invoke>",
            value
        );
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(&chunk));
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert_eq!(tools.len(), 1, "should synthesize 1 item tool_use: {:?}", tools);
        let parsed: serde_json::Value =
            serde_json::from_str(&tools[0].1).expect("input should be valid JSON");
        assert_eq!(
            parsed["content"], value,
            "parameter value should be fully preserved (including < / multi line / Chinese)"
        );
    }

    #[test]
    fn test_invoke_sniff_two_invokes_sequential() {
        // 🟢 2 item invoke chain → 2 item tool_use
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(
            "<invoke name=\"tool_a\"><parameter name=\"x\">1</parameter></invoke><invoke name=\"tool_b\"><parameter name=\"y\">2</parameter></invoke>",
        ));
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert_eq!(tools.len(), 2, "should synthesize 2 item tool_use: {:?}", tools);
        assert_eq!(tools[0].0, "tool_a");
        assert_eq!(tools[1].0, "tool_b");
    }

    #[test]
    fn test_invoke_sniff_split_across_chunks() {
        // 🟢 across chunk Sharded: the tag is cut apart and fed in multiple times.
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response("<inv"));
        all.extend(ctx.process_assistant_response("oke name=\"exec_command\">"));
        all.extend(ctx.process_assistant_response("<parameter name=\"cmd\">ls</parameter></in"));
        all.extend(ctx.process_assistant_response("voke>"));
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert_eq!(tools.len(), 1, "across chunk should synthesize 1 item tool_use: {:?}", tools);
        assert_eq!(tools[0].0, "exec_command");
        let parsed: serde_json::Value =
            serde_json::from_str(&tools[0].1).expect("input should be valid JSON");
        assert_eq!(parsed["cmd"], "ls");
    }

    #[test]
    fn test_invoke_sniff_strips_stray_call_token() {
        // 🟢 stray token:<invoke> beforehasa single oneline `call` → strip off,text no residue call
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(
            "call\n<invoke name=\"exec_command\"><parameter name=\"cmd\">ls</parameter></invoke>",
        ));
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert_eq!(tools.len(), 1, "should synthesize 1 item tool_use: {:?}", tools);

        let text = collect_text_content(&all);
        assert!(
            !text.contains("call"),
            "leading stray `call` should bestrip off,text should not remain: {:?}",
            text
        );
    }

    #[test]
    fn strip_trailing_stray_preserves_preceding_newline() {
        // regression:narrative textafterfollow oneline stray token(`some text\ncall`).
        // old implementation stray Strips the line together with the newline before it. -> obtain "some text"(no trailing newline),
        // this willletfollowafterof invoke_looks_like_real_leak The line start heuristic fails and misses a real leak.
        // correct:only strip stray the line itself, keeping the previous line newline. -> "some text\n".
        let got = strip_trailing_stray_tokens("some text\ncall");
        assert_eq!(
            got, "some text\n",
            "must keep the newline terminating the narrative line so the invoke stays line-start"
        );
        // And the stripped result should pass the line start judgment.
        assert!(
            invoke_looks_like_real_leak(got),
            "stripped narrative must still look like a line-start leak (ends with newline)"
        );
    }

    #[test]
    fn test_invoke_sniff_reclaims_after_narrative_then_stray_token() {
        // end to end:`body\ncall\n<invoke...>` —— body + stray token + real leak invoke.
        // oldimplementmissed retrieval(stray over stripping mixes the body text and invoke squeezed onto one line); after the fix recovery should succeed. tool_use.
        let mut ctx =
            StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(
            "first checkresult.\ncall\n<invoke name=\"exec_command\"><parameter name=\"cmd\">ls</parameter></invoke>",
        ));
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert_eq!(tools.len(), 1, "narrative+stray+invoke should recover 1 item tool_use: {:?}", tools);
        let text = collect_text_content(&all);
        assert!(text.contains("first checkresult"), "the narrative body should be kept: {:?}", text);
        assert!(!text.contains("call\n<invoke") && !text.contains("<invoke"), "invoke should not leak as text: {:?}", text);
    }

    #[test]
    fn test_invoke_sniff_keeps_narrative_before_invoke() {
        // 🟢 invoke beforehasnarrate:text contains"first check",1 item tool_use
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(
            "first check\n<invoke name=\"exec_command\"><parameter name=\"cmd\">ls</parameter></invoke>",
        ));
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert_eq!(tools.len(), 1, "should synthesize 1 item tool_use: {:?}", tools);

        let text = collect_text_content(&all);
        assert!(
            text.contains("first check"),
            "the narrative text should be kept in text in: {:?}",
            text
        );
    }

    #[test]
    fn test_invoke_sniff_truncated_block_not_captured() {
        // 🔴 truncated halfblock(no </invoke> closed)→ 0 tool_use
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(
            "<invoke name=\"exec_command\"><parameter name=\"cmd\">ls",
        ));
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert!(tools.is_empty(), "an unclosed block should not be captured: {:?}", tools);
    }

    #[test]
    fn test_invoke_midsentence_not_captured() {
        // 🔴 P1: embedded in the middle of a sentence in the body (no backtick, not line start), <invoke> is discussion text, should not be captured
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(
            "parser illustration: the model emits <invoke name=\"exec_command\"><parameter name=\"cmd\">ls</parameter></invoke> this text",
        ));
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert!(
            tools.is_empty(),
            "sentenceindiscussof <invoke> should not be captured: {:?}",
            tools
        );

        let text = collect_text_content(&all);
        assert!(
            text.contains("parserillustrate") && text.contains("this text"),
            "The body should be fully preserved (including narrative before and after).: {:?}",
            text
        );
        assert!(
            text.contains("<invoke name=\"exec_command\">"),
            "original <invoke> the text should be kept as is in text in: {:?}",
            text
        );
    }

    #[test]
    fn test_invoke_midsentence_unclosed_not_hold() {
        // 🔴 P2: encounters an unclosed mid sentence one during streaming. <invoke, should not hold capture the subsequent text to the end of the stream
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        // first time process: the unclosed one within a sentence <invoke>, there is body text earlier on the same line“discuss”
        let first = ctx.process_assistant_response("discuss <invoke name=\"x\"> semantics,");
        let first_text = collect_text_content(&first);
        assert!(
            first_text.contains("discuss"),
            "sentenceinnotclosedof <invoke should not hold capture the body text, should emit in time“discuss”: {:?}",
            first_text
        );

        let mut all = first;
        all.extend(ctx.process_assistant_response("afterfacecontent."));
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert!(
            tools.is_empty(),
            "notclosedofsentencein <invoke should not be captured: {:?}",
            tools
        );

        let text = collect_text_content(&all);
        assert!(
            text.contains("discuss") && text.contains("semantics") && text.contains("afterfacecontent."),
            "all body text should be fully preserved: {:?}",
            text
        );
    }

    #[test]
    fn test_invoke_multiline_patch_split_still_captured() {
        // 🟢 P3:line startvalid invoke,parametervalue is 20+ line multi line text (simulate apply_patch),
        // Feeds line by line in streaming. Before the fix the newline count ≥16 will be too_long Wrongly killed and downgraded to text; after the fix it should be caught.
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        // construct a 24 multi line patch content
        let mut patch_lines = Vec::new();
        for i in 0..24 {
            patch_lines.push(format!("+ line number {i} of the patch body"));
        }
        let patch_value = patch_lines.join("\n");

        // After the whole block is assembled, slices it by line and feeds piece by piece (each piece appends the newline back, the last line does not).
        let full = format!(
            "<invoke name=\"apply_patch\"><parameter name=\"input\">{}</parameter></invoke>",
            patch_value
        );
        let mut all = Vec::new();
        // Splits into pieces by newline and feeds piece by piece; ensures invoke Before all pieces arrive the newline count has already ≥16
        let bytes = full.as_bytes();
        let mut idx = 0;
        while idx < bytes.len() {
            // Finds the next newline boundary (including the newline) as one piece.
            let mut end = idx;
            while end < bytes.len() && bytes[end] != b'\n' {
                end += 1;
            }
            if end < bytes.len() {
                end += 1; // takeswaplinealso carry along
            }
            let piece = std::str::from_utf8(&bytes[idx..end]).unwrap();
            all.extend(ctx.process_assistant_response(piece));
            idx = end;
        }
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert_eq!(
            tools.len(),
            1,
            "multi line fed in fragments invoke should capture 1 item tool_use: {:?}",
            tools
        );
        assert_eq!(tools[0].0, "apply_patch", "name should be apply_patch");
        let parsed: serde_json::Value =
            serde_json::from_str(&tools[0].1).expect("input should be valid JSON");
        assert_eq!(
            parsed["input"], patch_value,
            "A multi line parameter value should be fully preserved (newlines not lost)."
        );
    }

    #[test]
    fn test_invoke_large_patch_split_captured() {
        // 🟢 P3: parameter value ~17KB Multi line, fed in shards, asserts it is caught. 1 item tool_use(in 256KB below the limit).
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        // each line ~70 bytes × 250 line ≈ 17KB
        let mut lines = Vec::new();
        for i in 0..250 {
            lines.push(format!(
                "+ patch content row {i:04} padding xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
            ));
        }
        let big_value = lines.join("\n");
        assert!(
            big_value.len() > 16 * 1024,
            "testdatashould >16KB, actual {}",
            big_value.len()
        );

        let full = format!(
            "<invoke name=\"apply_patch\"><parameter name=\"input\">{}</parameter></invoke>",
            big_value
        );
        // fixed 512 fed in one byte per fragment (note UTF-8 boundary, here the content is ASCII safe)
        let mut all = Vec::new();
        let bytes = full.as_bytes();
        let mut idx = 0;
        while idx < bytes.len() {
            let end = (idx + 512).min(bytes.len());
            let piece = std::str::from_utf8(&bytes[idx..end]).unwrap();
            all.extend(ctx.process_assistant_response(piece));
            idx = end;
        }
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert_eq!(
            tools.len(),
            1,
            "~17KB feed in shardsof invoke should capture 1 item tool_use: {:?}",
            tools.iter().map(|t| &t.0).collect::<Vec<_>>()
        );
        assert_eq!(tools[0].0, "apply_patch");
        let parsed: serde_json::Value =
            serde_json::from_str(&tools[0].1).expect("input should be valid JSON");
        assert_eq!(parsed["input"], big_value, "large patch the parameter value should be fully preserved");
    }

    #[test]
    fn test_unclosed_invoke_eventually_flushed_as_text() {
        // 🟢 The locked byte fallback is still in place: line start `<invoke>` never closes, fed in more than MAX_INVOKE_HOLD_BYTES,
        // Should be emitted as text (not infinite hold).
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        // Line start open tag, never closed; fills plain text exceeding the limit (no </invoke>)
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response("<invoke name=\"x\">"));
        // Feeds in content exceeding the limit at once (using one without `<` padding, avoiding triggering other paths)
        let filler = "A".repeat(StreamContext::MAX_INVOKE_HOLD_BYTES + 1024);
        all.extend(ctx.process_assistant_response(&filler));
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert!(
            tools.is_empty(),
            "forevernotclosedof invoke should not be captured: {:?}",
            tools.len()
        );

        let text = collect_text_content(&all);
        assert!(
            text.contains("<invoke name=\"x\">"),
            "An unclosed block over the limit should be emitted as text (including the open tag)."
        );
        assert!(
            text.contains(&"A".repeat(100)),
            "The padding text should be emitted and should not be infinite. hold"
        );
    }

    #[test]
    fn test_invoke_in_markdown_list_not_captured() {
        // 🔴 markdown list item `- <invoke>` treated as discussion text, do not capture.
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(
            "- <invoke name=\"exec_command\"><parameter name=\"cmd\">rm -rf /</parameter></invoke>",
        ));
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert!(
            tools.is_empty(),
            "markdown in the list <invoke> should not be captured: {:?}",
            tools
        );
        let text = collect_text_content(&all);
        assert!(
            text.contains("rm -rf /"),
            "A dangerous command should stay in the text and not be executed.: {:?}",
            text
        );
    }

    #[test]
    fn test_invoke_in_blockquote_not_captured() {
        // 🔴 reference `> <invoke>` treated as discussion text, do not capture.
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(
            "> <invoke name=\"exec_command\"><parameter name=\"cmd\">rm -rf /</parameter></invoke>",
        ));
        all.extend(ctx.generate_final_events());

        let tools = collect_tool_uses(&all);
        assert!(
            tools.is_empty(),
            "referenceblockinside <invoke> should not be captured: {:?}",
            tools
        );
        let text = collect_text_content(&all);
        assert!(
            text.contains("rm -rf /"),
            "A dangerous command should stay in the text and not be executed.: {:?}",
            text
        );
    }

    fn block_start_position(events: &[SseEvent], block_type: &str) -> (usize, i64) {
        let pos = events
            .iter()
            .position(|e| {
                e.event == "content_block_start" && e.data["content_block"]["type"] == block_type
            })
            .unwrap_or_else(|| panic!("{block_type} block should start"));
        let idx = events[pos].data["index"]
            .as_i64()
            .unwrap_or_else(|| panic!("{block_type} block index should exist"));
        (pos, idx)
    }

    fn block_stop_position(events: &[SseEvent], index: i64) -> usize {
        events
            .iter()
            .position(|e| e.event == "content_block_stop" && e.data["index"].as_i64() == Some(index))
            .unwrap_or_else(|| panic!("block {index} should stop"))
    }

    #[test]
    fn test_end_tag_newlines_split_across_events() {
        // `</thinking>\n` in chunk 1,`\n` in chunk 2,`text` in chunk 3
        // ensure `</thinking>` will not be partly treated as thinking emit content
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _initial_events = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response("<thinking>\nabc</thinking>\n"));
        all.extend(ctx.process_assistant_response("\n"));
        all.extend(ctx.process_assistant_response("hello"));
        all.extend(ctx.generate_final_events());

        let thinking = collect_thinking_content(&all);
        assert_eq!(
            thinking, "abc",
            "thinking should be 'abc', got: {:?}",
            thinking
        );

        let text = collect_text_content(&all);
        assert_eq!(text, "hello", "text should be 'hello', got: {:?}", text);
    }

    #[test]
    fn test_end_tag_alone_in_chunk_then_newlines_in_next() {
        // `</thinking>` separateina chunk,`\n\ntext` in the next chunk
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _initial_events = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response("<thinking>\nabc</thinking>"));
        all.extend(ctx.process_assistant_response("\n\nhello"));
        all.extend(ctx.generate_final_events());

        let thinking = collect_thinking_content(&all);
        assert_eq!(
            thinking, "abc",
            "thinking should be 'abc', got: {:?}",
            thinking
        );

        let text = collect_text_content(&all);
        assert_eq!(text, "hello", "text should be 'hello', got: {:?}", text);
    }

    #[test]
    fn test_start_tag_newline_split_across_events() {
        // `\n\n` in chunk 1,`<thinking>` in chunk 2,`\n` in chunk 3
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _initial_events = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response("\n\n"));
        all.extend(ctx.process_assistant_response("<thinking>"));
        all.extend(ctx.process_assistant_response("\n"));
        all.extend(ctx.process_assistant_response("abc</thinking>\n\ntext"));
        all.extend(ctx.generate_final_events());

        let thinking = collect_thinking_content(&all);
        assert_eq!(
            thinking, "abc",
            "thinking should be 'abc', got: {:?}",
            thinking
        );

        let text = collect_text_content(&all);
        assert_eq!(text, "text", "text should be 'text', got: {:?}", text);
    }

    #[test]
    fn test_full_flow_maximally_split() {
        // Extreme split: every key boundary is in a different chunk
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _initial_events = ctx.generate_initial_events();

        let mut all = Vec::new();
        // \n\n<thinking>\n split into segments
        all.extend(ctx.process_assistant_response("\n"));
        all.extend(ctx.process_assistant_response("\n"));
        all.extend(ctx.process_assistant_response("<thin"));
        all.extend(ctx.process_assistant_response("king>"));
        all.extend(ctx.process_assistant_response("\n"));
        all.extend(ctx.process_assistant_response("hello"));
        // </thinking>\n\n split into segments
        all.extend(ctx.process_assistant_response("</thi"));
        all.extend(ctx.process_assistant_response("nking>"));
        all.extend(ctx.process_assistant_response("\n"));
        all.extend(ctx.process_assistant_response("\n"));
        all.extend(ctx.process_assistant_response("world"));
        all.extend(ctx.generate_final_events());

        let thinking = collect_thinking_content(&all);
        assert_eq!(
            thinking, "hello",
            "thinking should be 'hello', got: {:?}",
            thinking
        );

        let text = collect_text_content(&all);
        assert_eq!(text, "world", "text should be 'world', got: {:?}", text);
    }

    #[test]
    fn test_thinking_only_sets_max_tokens_stop_reason() {
        // wholestreamonly thinking block, none text also none tool_use,stop_reason should be max_tokens
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _initial_events = ctx.generate_initial_events();

        let mut all_events = Vec::new();
        all_events.extend(ctx.process_assistant_response("<thinking>\nabc</thinking>"));
        all_events.extend(ctx.generate_final_events());

        let message_delta = all_events
            .iter()
            .find(|e| e.event == "message_delta")
            .expect("should have message_delta event");

        assert_eq!(
            message_delta.data["delta"]["stop_reason"], "max_tokens",
            "stop_reason should be max_tokens when only thinking is produced"
        );

        // should resend a complete set of text event (content_block_start + delta space + content_block_stop)
        assert!(
            all_events.iter().any(|e| {
                e.event == "content_block_start" && e.data["content_block"]["type"] == "text"
            }),
            "should emit text content_block_start"
        );
        assert!(
            all_events.iter().any(|e| {
                e.event == "content_block_delta"
                    && e.data["delta"]["type"] == "text_delta"
                    && e.data["delta"]["text"] == " "
            }),
            "should emit text_delta with a single space"
        );
        // text block should be generate_final_events auto close
        let text_block_index = all_events
            .iter()
            .find_map(|e| {
                if e.event == "content_block_start" && e.data["content_block"]["type"] == "text" {
                    e.data["index"].as_i64()
                } else {
                    None
                }
            })
            .expect("text block should exist");
        assert!(
            all_events.iter().any(|e| {
                e.event == "content_block_stop"
                    && e.data["index"].as_i64() == Some(text_block_index)
            }),
            "text block should be stopped"
        );
    }

    #[test]
    fn test_thinking_with_text_keeps_end_turn_stop_reason() {
        // thinking + text case,stop_reason should be end_turn
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _initial_events = ctx.generate_initial_events();

        let mut all_events = Vec::new();
        all_events.extend(ctx.process_assistant_response("<thinking>\nabc</thinking>\n\nHello"));
        all_events.extend(ctx.generate_final_events());

        let message_delta = all_events
            .iter()
            .find(|e| e.event == "message_delta")
            .expect("should have message_delta event");

        assert_eq!(
            message_delta.data["delta"]["stop_reason"], "end_turn",
            "stop_reason should be end_turn when text is also produced"
        );
    }

    #[test]
    fn test_thinking_with_tool_use_keeps_tool_use_stop_reason() {
        // thinking + tool_use case,stop_reason should be tool_use
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), test_known_tools());
        let _initial_events = ctx.generate_initial_events();

        let mut all_events = Vec::new();
        all_events.extend(ctx.process_assistant_response("<thinking>\nabc</thinking>"));
        all_events.extend(
            ctx.process_tool_use(&crate::kiro::model::events::ToolUseEvent {
                name: "test_tool".to_string(),
                tool_use_id: "tool_1".to_string(),
                input: "{}".to_string(),
                stop: true,
            }),
        );
        all_events.extend(ctx.generate_final_events());

        let message_delta = all_events
            .iter()
            .find(|e| e.event == "message_delta")
            .expect("should have message_delta event");

        assert_eq!(
            message_delta.data["delta"]["stop_reason"], "tool_use",
            "stop_reason should be tool_use when tool_use is present"
        );
    }

    // ===== add a regression test:P0-1 parametercontains literal XML / 🅱 code fence / 🅳 tool table / 🅲 card =====

    /// 🅿️ P0-1: the parameter value contains a literal `</invoke>`, the block should not be truncated by a false close,input must be complete.
    #[test]
    fn test_invoke_param_value_contains_literal_invoke_close() {
        let mut ctx =
            StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();
        // patch a literal appears in the body </invoke>, the real close is at the end
        let payload = "count\n<invoke name=\"apply_patch\"><parameter name=\"input\">line1\n</invoke>\nstill in patch\nline3</parameter></invoke>";
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(payload));
        all.extend(ctx.generate_final_events());
        let tools = collect_tool_uses(&all);
        assert_eq!(tools.len(), 1, "should synthesize 1 item tool_use: {:?}", tools);
        assert_eq!(tools[0].0, "apply_patch");
        let parsed: serde_json::Value = serde_json::from_str(&tools[0].1).expect("valid JSON");
        let input = parsed["input"].as_str().expect("has input");
        assert!(input.contains("still in patch"), "input should not be truncated by a false close: {input:?}");
        assert!(input.contains("line3"), "input should contain line3: {input:?}");
        let text = collect_text_content(&all);
        assert!(!text.contains("still in patch"), "patch the body should not leak into text: {text:?}");
    }

    /// 🅿️ P0-1: the parameter value contains a literal `</parameter>`, the value should not be truncated losing its second half.
    #[test]
    fn test_invoke_param_value_contains_literal_parameter_close() {
        let mut ctx =
            StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();
        let payload = "count\n<invoke name=\"apply_patch\"><parameter name=\"input\">before</parameter> after the fake close</parameter></invoke>";
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(payload));
        all.extend(ctx.generate_final_events());
        let tools = collect_tool_uses(&all);
        assert_eq!(tools.len(), 1, "should synthesize 1 item tool_use: {:?}", tools);
        let parsed: serde_json::Value = serde_json::from_str(&tools[0].1).expect("valid JSON");
        let input = parsed["input"].as_str().expect("has input");
        assert!(input.contains("after the fake close"), "afterhalfsegmentshould notdrop: {input:?}");
    }

    /// 🅱:code fence(```) inside <invoke> is body display, should not be recovered into tool_use.
    #[test]
    fn test_invoke_inside_code_fence_not_captured() {
        let mut ctx =
            StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();
        let payload = "examplecode:\n```\n<invoke name=\"exec_command\"><parameter name=\"cmd\">rm -rf /</parameter></invoke>\n```\nexplanation complete.";
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(payload));
        all.extend(ctx.generate_final_events());
        let tools = collect_tool_uses(&all);
        assert!(tools.is_empty(), "Text shown inside a fence should not be recovered.: {:?}", tools);
        let text = collect_text_content(&all);
        assert!(text.contains("<invoke name=\"exec_command\">"), "shouldoriginalsampleretain: {text:?}");
    }

    /// 🅳: the synthesized tool name is not in the known tool table. → Not recovered, emitted as text (prevents wrong execution).
    #[test]
    fn test_invoke_unknown_tool_name_not_synthesized() {
        // not in the known tool table totally_unknown_tool
        let mut ctx =
            StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();
        let payload = "count\n<invoke name=\"totally_unknown_tool\"><parameter name=\"x\">1</parameter></invoke>";
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(payload));
        all.extend(ctx.generate_final_events());
        let tools = collect_tool_uses(&all);
        assert!(tools.is_empty(), "an unknown tool name should not be synthesized: {:?}", tools);
        let text = collect_text_content(&all);
        assert!(text.contains("totally_unknown_tool"), "an unknown tool should be treated as text as is: {text:?}");
    }

    /// 🅳: the known tool table is empty (the request did not carry tools)→ Never recovered; better to miss than to wrongly execute.
    #[test]
    fn test_invoke_empty_known_tools_never_captured() {
        let mut ctx = StreamContext::new_with_thinking(
            "test-model",
            1,
            false,
            HashMap::new(),
            std::collections::HashSet::new(),
        );
        let _ = ctx.generate_initial_events();
        let payload = "count\n<invoke name=\"exec_command\"><parameter name=\"cmd\">ls</parameter></invoke>";
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(payload));
        all.extend(ctx.generate_final_events());
        let tools = collect_tool_uses(&all);
        assert!(tools.is_empty(), "when the tool table is empty nothing should be recovered: {:?}", tools);
    }

    /// 🅲:stray token `card` should also be stripped, the block is still recovered.
    #[test]
    fn test_invoke_strips_stray_card_token() {
        let mut ctx =
            StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();
        let payload = "I firstetc.result.\n\ncard\n<invoke name=\"wait_agent\"><parameter name=\"x\">1</parameter></invoke>";
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(payload));
        all.extend(ctx.generate_final_events());
        let tools = collect_tool_uses(&all);
        assert_eq!(tools.len(), 1, "card the prefixed block should be recovered: {:?}", tools);
        assert_eq!(tools[0].0, "wait_agent");
        let text = collect_text_content(&all);
        assert!(!text.contains("card"), "card stray token should not leak: {text:?}");
        assert!(text.contains("I firstetc.result"), "normal narration should be kept: {text:?}");
    }

    /// 🅱 across chunk:``` fenceopening tagin chunk Even with the boundary cut apart, still correctly identifies not to recover inside a fence.
    #[test]
    fn test_invoke_fence_split_across_chunks() {
        let mut ctx =
            StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();
        let mut all = Vec::new();
        // the fence opening tag is split into two chunk arrive
        all.extend(ctx.process_assistant_response("see code:\n``"));
        all.extend(ctx.process_assistant_response("`\n<invoke name=\"exec_command\"><parameter name=\"cmd\">x</parameter></invoke>\n```"));
        all.extend(ctx.generate_final_events());
        let tools = collect_tool_uses(&all);
        assert!(tools.is_empty(), "across chunk should not recover within the fence: {:?}", tools);
    }

    /// 🟡 regression (Reviewer issue1): send consecutively burst, block A in `</invoke>` mixed non before `>` closing text,
    /// should not A,B wrongly merged into one block, nor should it let B parameterstringenter A. Both blocks should be recovered independently.
    #[test]
    fn test_invoke_burst_with_trailing_text_not_merged() {
        let mut ctx =
            StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();
        let payload = "count\n<invoke name=\"tool_a\"><parameter name=\"x\">1</parameter>trailing plain</invoke><invoke name=\"tool_b\"><parameter name=\"y\">2</parameter></invoke>";
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(payload));
        all.extend(ctx.generate_final_events());
        let tools = collect_tool_uses(&all);
        assert_eq!(tools.len(), 2, "shouldindependentsynthesize 2 item tool_use,notcanincorrect merge: {:?}", tools);
        assert_eq!(tools[0].0, "tool_a");
        assert_eq!(tools[1].0, "tool_b");
        let a: serde_json::Value = serde_json::from_str(&tools[0].1).expect("valid JSON");
        let b: serde_json::Value = serde_json::from_str(&tools[1].1).expect("valid JSON");
        assert!(a.get("y").is_none(), "B parameter y should not leak into A: {a:?}");
        assert_eq!(a["x"], "1");
        assert_eq!(b["y"], "2");
    }

    /// 🟢 normal consecutive send burst(blockclosely attached,A to </parameter> closing) should still be correctly split into two.
    #[test]
    fn test_invoke_burst_clean_two_blocks() {
        let mut ctx =
            StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();
        let payload = "count\n<invoke name=\"tool_a\"><parameter name=\"x\">1</parameter></invoke><invoke name=\"tool_b\"><parameter name=\"y\">2</parameter></invoke>";
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(payload));
        all.extend(ctx.generate_final_events());
        let tools = collect_tool_uses(&all);
        assert_eq!(tools.len(), 2, "closely consecutive should be split into 2 item: {:?}", tools);
        assert_eq!(tools[0].0, "tool_a");
        assert_eq!(tools[1].0, "tool_b");
    }

    /// 🔁 replay validation: use the problem thread `019e9e8d` real inside `count\n<invoke>` leakoriginal text,
    /// Asserts the new fault tolerance recovers it into structured. tool_use(rather than leaking as a literal XML text).
    /// realtoolname exec_command intool tablein → should recover;parameter cmd / yield_time_ms complete.
    #[test]
    fn test_invoke_real_leak_sample_from_thread_019e9e8d() {
        let known: std::collections::HashSet<String> =
            ["exec_command", "update_plan", "update_goal"].iter().map(|s| s.to_string()).collect();
        let mut ctx =
            StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), known);
        let _ = ctx.generate_initial_events();
        // verbatim from thread 019e9e8d real leak assistant message
        let real = ").\n\ncount\n<invoke name=\"exec_command\">\n<parameter name=\"cmd\">cd /Users/yuyifeng/.codex/everything-codex/runtime/agent-tools && python3 -m pytest -q -p no:cacheprovider objects/dev/beads/leaves/create_issue/ 2>&1 | tail -8</parameter>\n<parameter name=\"yield_time_ms\">60000</parameter>\n</invoke>";
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(real));
        all.extend(ctx.generate_final_events());
        let tools = collect_tool_uses(&all);
        assert_eq!(tools.len(), 1, "A real leak sample should be recovered into 1 item tool_use: {:?}", tools);
        assert_eq!(tools[0].0, "exec_command", "name should be exec_command");
        let parsed: serde_json::Value =
            serde_json::from_str(&tools[0].1).expect("input should be valid JSON");
        assert!(
            parsed["cmd"].as_str().unwrap_or("").contains("pytest"),
            "cmd the parameter should be fully preserved: {:?}", parsed
        );
        assert_eq!(parsed["yield_time_ms"], "60000", "yield_time_ms parametershouldretain");
        // key:literal <invoke> should not leakto text
        let text = collect_text_content(&all);
        assert!(
            !text.contains("<invoke name=\"exec_command\">"),
            "literal <invoke> should not leak into text: {:?}", text
        );
        // count stray token alsoshould not leak
        assert!(!text.contains("\ncount\n") && !text.ends_with("count"),
            "count stray token should not leak: {:?}", text);
    }

    // ---- repeat circuit breaker (repeat guard):root cause = Opus long context degradation repeat readout ----

    /// 🔴→🟢 Reproduces a real leak: the model repeats infinitely after one normal sentence. `count`(thread 019ea4e9 oftrueaccount).
    /// circuit breakafteremitof count The count must be far smaller than the amount fed in, and must not fill the output.
    #[test]
    fn repeat_guard_trips_on_count_flood() {
        let mut ctx =
            StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();

        // real form: normal speech + call + massive count(here use 5000 times simulate 3.2 ten thousand times)
        let mut payload = String::from("first check crawlee state.\n\ncall\n\n");
        for _ in 0..5000 {
            payload.push_str("count\n\n");
        }
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(&payload));
        all.extend(ctx.generate_final_events());

        let text = collect_text_content(&all);
        let emitted_counts = text.matches("count").count();
        assert!(
            emitted_counts < 64,
            "repeat readout should be broken: the emitted count the count should be far smaller than the fed in 5000, actual={}",
            emitted_counts
        );
        // The normal opening sentence must be kept (circuit breaking must not harm the body).
        assert!(
            text.contains("first check crawlee state"),
            "the circuit breaker should not harm normal body text: {:?}",
            &text[..text.len().min(80)]
        );
    }

    /// 🟢 No wrongful harm: the one before a normal tool call. 1 guide word `count` + true <invoke> is still normally recovered.
    #[test]
    fn repeat_guard_does_not_trip_on_single_stray_token() {
        let mut ctx =
            StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();
        let payload =
            "count\n<invoke name=\"exec_command\"><parameter name=\"cmd\">ls</parameter></invoke>";
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(payload));
        all.extend(ctx.generate_final_events());
        let tools = collect_tool_uses(&all);
        assert_eq!(tools.len(), 1, "A single guide word should not trip the breaker,invoke shouldnormally retrieved: {:?}", tools);
        assert_eq!(tools[0].0, "exec_command");
    }

    /// 🟢 No wrongful harm: occasionally appears in normal multi line text. count A single word (not a standalone line repeat) does not trip the breaker.
    #[test]
    fn repeat_guard_does_not_trip_on_normal_prose() {
        let mut ctx =
            StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();
        let payload = "Icountbriefly count = 3, then continue doing other things.\nthis is the second line of normal text.\nthe third line is also normal.";
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response(payload));
        all.extend(ctx.generate_final_events());
        let text = collect_text_content(&all);
        assert!(text.contains("Icountbriefly"), "normal body text should not be broken by the circuit breaker: {:?}", text);
        assert!(text.contains("numberthreelinealso normal"), "normal body text should be fully preserved: {:?}", text);
    }

    /// 🟢 across chunk Repeats can also be circuit broken (streaming pieces arrive, one per piece). count).
    #[test]
    fn repeat_guard_trips_across_chunks() {
        let mut ctx =
            StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), test_known_tools());
        let _ = ctx.generate_initial_events();
        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response("call\n\n"));
        for _ in 0..2000 {
            all.extend(ctx.process_assistant_response("count\n\n"));
        }
        all.extend(ctx.generate_final_events());
        let text = collect_text_content(&all);
        let emitted_counts = text.matches("count").count();
        assert!(
            emitted_counts < 64,
            "across chunk Repeats should also trip the breaker: actually emitted count={}",
            emitted_counts
        );
    }

    // ---- blocklevelrepeat circuit breaker (collapse_stray_token_floods): cover web_search loop path ----

    /// 🔴→🟢 blocklevelpath (extract_invoke_content_blocks / web_search loop)must also trip the circuit breaker count flood.
    #[test]
    fn extract_blocks_collapses_count_flood() {
        let mut text = String::from("first check crawlee state.\n\ncall\n\n");
        for _ in 0..5000 {
            text.push_str("count\n\n");
        }
        let blocks = extract_invoke_content_blocks(
            &text,
            &test_known_tools(),
            &std::collections::HashMap::new(),
        );
        let joined: String = blocks
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect();
        let emitted = joined.matches("count").count();
        assert!(emitted < 64, "the block level path should fold count flood:actual={}", emitted);
        assert!(joined.contains("first check crawlee state"), "normal body text should be kept: {:?}", &joined[..joined.len().min(60)]);
    }

    /// 🟢 Block level no wrongful harm: a single guide word. count + true invoke stillbyretrieve.
    #[test]
    fn extract_blocks_keeps_single_stray_and_reclaims() {
        let text = "count\n<invoke name=\"exec_command\">\n<parameter name=\"cmd\">ls</parameter>\n</invoke>";
        let blocks = extract_invoke_content_blocks(
            text,
            &test_known_tools(),
            &std::collections::HashMap::new(),
        );
        assert!(
            blocks.iter().any(|b| b["type"] == "tool_use" && b["name"] == "exec_command"),
            "A single guide word should not trigger folding,invoke should recover: {:?}",
            blocks
        );
    }

    #[test]
    fn test_native_reasoning_event_emits_thinking_with_signature() {
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), std::collections::HashSet::new());
        let mut all_events = ctx.generate_initial_events();

        all_events.extend(ctx.process_kiro_event(&Event::ReasoningContent(
            crate::kiro::model::events::ReasoningContentEvent {
                text: Some("native reasoning".to_string()),
                signature: Some("real-signature".to_string()),
                redacted_content: None,
            },
        )));
        all_events.extend(ctx.process_assistant_response("final answer"));
        all_events.extend(ctx.generate_final_events());

        assert_eq!(collect_thinking_content(&all_events), "native reasoning");
        assert_eq!(collect_text_content(&all_events), "final answer");
        assert!(all_events.iter().any(|e| {
            e.event == "content_block_delta"
                && e.data["delta"]["type"] == "signature_delta"
                && e.data["delta"]["signature"] == "real-signature"
        }));
    }

    #[test]
    fn test_native_reasoning_signature_only_applies_to_next_thinking_text() {
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), std::collections::HashSet::new());
        let mut all_events = ctx.generate_initial_events();

        all_events.extend(ctx.process_kiro_event(&Event::ReasoningContent(
            crate::kiro::model::events::ReasoningContentEvent {
                text: None,
                signature: Some("signature-before-text".to_string()),
                redacted_content: None,
            },
        )));
        all_events.extend(ctx.process_kiro_event(&Event::ReasoningContent(
            crate::kiro::model::events::ReasoningContentEvent {
                text: Some("delayed native reasoning".to_string()),
                signature: None,
                redacted_content: None,
            },
        )));
        all_events.extend(ctx.generate_final_events());

        assert_eq!(collect_thinking_content(&all_events), "delayed native reasoning");
        assert!(all_events.iter().any(|e| {
            e.event == "content_block_delta"
                && e.data["delta"]["type"] == "signature_delta"
                && e.data["delta"]["signature"] == "signature-before-text"
        }));
    }

    #[test]
    fn test_native_reasoning_text_downgrades_to_text_when_thinking_disabled() {
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false, HashMap::new(), std::collections::HashSet::new());
        let mut all_events = ctx.generate_initial_events();

        all_events.extend(ctx.process_kiro_event(&Event::ReasoningContent(
            crate::kiro::model::events::ReasoningContentEvent {
                text: Some("visible reasoning fallback".to_string()),
                signature: Some("ignored-signature".to_string()),
                redacted_content: Some("ignored-redacted".to_string()),
            },
        )));
        all_events.extend(ctx.generate_final_events());

        assert_eq!(collect_text_content(&all_events), "visible reasoning fallback");
        assert_eq!(collect_thinking_content(&all_events), "");
        assert!(!all_events.iter().any(|e| {
            e.event == "content_block_delta" && e.data["delta"]["type"] == "signature_delta"
        }));
        assert!(!all_events.iter().any(|e| {
            e.event == "content_block_start"
                && e.data["content_block"]["type"] == "redacted_thinking"
        }));
    }

    #[test]
    fn test_native_redacted_thinking_is_ordered_between_thinking_and_text() {
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), std::collections::HashSet::new());
        let mut all_events = ctx.generate_initial_events();

        all_events.extend(ctx.process_kiro_event(&Event::ReasoningContent(
            crate::kiro::model::events::ReasoningContentEvent {
                text: Some("native reasoning".to_string()),
                signature: Some("real-signature".to_string()),
                redacted_content: None,
            },
        )));
        all_events.extend(ctx.process_kiro_event(&Event::ReasoningContent(
            crate::kiro::model::events::ReasoningContentEvent {
                text: None,
                signature: None,
                redacted_content: Some("encrypted-thinking".to_string()),
            },
        )));
        all_events.extend(ctx.process_assistant_response("final answer"));
        all_events.extend(ctx.generate_final_events());

        let (_, thinking_idx) = block_start_position(&all_events, "thinking");
        let thinking_stop_pos = block_stop_position(&all_events, thinking_idx);
        let (redacted_start_pos, redacted_idx) =
            block_start_position(&all_events, "redacted_thinking");
        let redacted_stop_pos = block_stop_position(&all_events, redacted_idx);
        let (text_start_pos, _) = block_start_position(&all_events, "text");

        assert!(
            thinking_stop_pos < redacted_start_pos,
            "thinking block must close before redacted_thinking starts"
        );
        assert!(
            redacted_stop_pos < text_start_pos,
            "redacted_thinking block must close before text starts"
        );
        assert_eq!(collect_thinking_content(&all_events), "native reasoning");
        assert_eq!(collect_text_content(&all_events), "final answer");
    }

    #[test]
    fn test_native_reasoning_event_emits_redacted_thinking() {
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true, HashMap::new(), std::collections::HashSet::new());
        let mut all_events = ctx.generate_initial_events();

        all_events.extend(ctx.process_kiro_event(&Event::ReasoningContent(
            crate::kiro::model::events::ReasoningContentEvent {
                text: None,
                signature: None,
                redacted_content: Some("encrypted-thinking".to_string()),
            },
        )));
        all_events.extend(ctx.generate_final_events());

        assert!(all_events.iter().any(|e| {
            e.event == "content_block_start"
                && e.data["content_block"]["type"] == "redacted_thinking"
                && e.data["content_block"]["data"] == "encrypted-thinking"
        }));
    }
}
