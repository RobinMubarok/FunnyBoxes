//! Compiler FunnyBoxes (`.fny`).
//!
//! Tahap saat ini: lexer, parser, AST, backend JavaScript (app.js),
//! dan keluaran index.html serta style.css. WebAssembly (core.wasm) belum ada.

pub mod ast;
pub mod base64;
pub mod codegen;
pub mod error;
pub mod lexer;
pub mod output;
pub mod parser;
pub mod wasm_emit;
pub mod wasm_encode;

use ast::Program;
use error::{Diag, SourceFile};

/// Mengurai sumber FunnyBoxes menjadi AST.
pub fn parse(src: &str) -> Result<Program, Diag> {
    let mut p = parser::Parser::new(src);
    p.parse_program()
}

/// Mengurai file dan mengembalikan diagnostik yang sudah diformat dengan lokasi.
pub fn parse_file(file: &SourceFile) -> Result<Program, String> {
    parse(&file.src).map_err(|d| file.render(&d))
}
