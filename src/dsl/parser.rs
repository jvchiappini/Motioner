/// Parser for the Motioner DSL.
use std::collections::HashMap;
use super::ast::{HeaderConfig, MoveBlock, Statement};
use super::lexer::extract_balanced;
use crate::dsl::utils;
use crate::scene::{Shape, Easing};

pub fn parse(src: &str) -> Vec<Statement> {
    let mut stmts = Vec::new();
    let mut lines = src.lines().map(str::trim).peekable();
    let mut pending_moves: Vec<MoveBlock> = Vec::new();

    while let Some(line) = lines.next() {
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        if line.starts_with("size") || line.starts_with("timeline") {
            continue;
        }

        if line.starts_with("move") && line.contains('{') {
            let block = collect_block(line, &mut lines);
            if let Some(mv) = parse_move_block_lines(&block) {
                if mv.element.is_some() {
                    pending_moves.push(mv);
                }
            }
            continue;
        }

        if line.contains('{') {
            let ident = first_ident(line);
            if ident == "rect" {
                let block = collect_block(line, &mut lines);
                if let Some(shape) = parse_rect_block(&block) {
                    stmts.push(Statement::Shape(shape));
                    continue;
                }
            }
        }
    }

    for mv in pending_moves {
        stmts.push(Statement::Move(mv));
    }

    stmts
}

pub fn parse_config(src: &str) -> Result<HeaderConfig, String> {
    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;
    let mut fps: Option<u32> = None;
    let mut duration: Option<f32> = None;

    if let Some(pos) = src.find("size") {
        if let Some(inner) = extract_balanced(src, pos, '(', ')') {
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() == 2 {
                width = parts[0].trim().parse().ok();
                height = parts[1].trim().parse().ok();
            }
        }
    }

    if let Some(pos) = src.find("timeline") {
        let inner = extract_balanced(src, pos, '(', ')')
            .or_else(|| extract_balanced(src, pos, '{', '}'))
            .unwrap_or_default();

        let sep = if inner.contains(';') { ';' } else { ',' };
        for part in inner.split(sep) {
            let s = part.trim();
            if let Some(v) = utils::parse_named_value(s, "fps") {
                fps = v.parse::<u32>().ok();
            }
            if let Some(v) = utils::parse_named_value(s, "duration") {
                duration = v.parse::<f32>().ok();
            }
        }
    }

    match (width, height, fps, duration) {
        (Some(w), Some(h), Some(f), Some(d)) => Ok(HeaderConfig {
            width: w,
            height: h,
            fps: f,
            duration: d,
        }),
        _ => Err("Missing basic configuration (size, timeline)".to_string()),
    }
}

pub fn parse_move_block_lines(block: &[String]) -> Option<MoveBlock> {
    let mut element = None;
    let mut to = None;
    let mut start_time = None;
    let mut end_time = None;
    let mut easing = Easing::Linear;

    for line in body_lines(block) {
        let line = line.trim();
        let Some((key, val)) = split_kv(line) else { continue };

        match key.as_str() {
            "element" => element = Some(val.trim_matches('"').to_string()),
            "to" => to = utils::parse_point(&val),
            "during" => {
                if let Some(arrow) = val.find("->") {
                    start_time = val[..arrow].trim().parse().ok();
                    end_time = val[arrow + 2..].trim().parse().ok();
                }
            }
            "ease" | "easing" => easing = utils::parse_easing(&val),
            _ => {}
        }
    }

    Some(MoveBlock {
        element,
        to: to?,
        during: (start_time?, end_time?),
        easing,
    })
}

fn parse_rect_block(block: &[String]) -> Option<Shape> {
    let name = extract_name(&block[0])?;
    let mut x = 0.5;
    let mut y = 0.5;
    let mut w = 0.1;
    let mut h = 0.1;
    let mut color = [255, 255, 255, 255];

    for line in body_lines(block) {
        let line = line.trim();
        let Some((key, val)) = split_kv(line) else { continue };
        match key.as_str() {
            "x" => x = val.parse().unwrap_or(x),
            "y" => y = val.parse().unwrap_or(y),
            "w" => w = val.parse().unwrap_or(w),
            "h" => h = val.parse().unwrap_or(h),
            "color" => {
                if let Some(c) = crate::dsl::ast::Color::from_hex(&val) {
                    color = c.to_array();
                }
            }
            _ => {}
        }
    }

    Some(Shape::Rect { name, x, y, w, h, color })
}

fn collect_block<'a, I>(header: &str, lines: &mut std::iter::Peekable<I>) -> Vec<String>
where
    I: Iterator<Item = &'a str>,
{
    let mut block = vec![header.to_string()];
    let mut depth = header.chars().filter(|&c| c == '{').count() as i32
        - header.chars().filter(|&c| c == '}').count() as i32;

    while depth > 0 {
        if let Some(line) = lines.next() {
            depth += line.chars().filter(|&c| c == '{').count() as i32;
            depth -= line.chars().filter(|&c| c == '}').count() as i32;
            block.push(line.to_string());
        } else {
            break;
        }
    }
    block
}

pub(crate) fn body_lines(block: &[String]) -> Vec<String> {
    let mut in_body = false;
    let mut depth = 0;
    let mut result = Vec::new();

    for line in block {
        for ch in line.chars() {
            match ch {
                '{' => { in_body = true; depth += 1; }
                '}' => { depth -= 1; }
                _ => {}
            }
        }
        if in_body && depth >= 1 {
            result.push(line.clone());
        }
    }
    result.retain(|l| l.trim() != "{");
    result
}

pub(crate) fn extract_name(header: &str) -> Option<String> {
    let start = header.find('"')?;
    let rest = &header[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn first_ident(s: &str) -> String {
    s.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect()
}

pub(crate) fn split_kv(s: &str) -> Option<(String, String)> {
    utils::split_kv(s)
}

pub fn method_color(_name: &str) -> Option<[u8; 4]> { None }
