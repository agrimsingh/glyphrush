mod backend;
#[cfg(feature = "pdfium")]
mod backend_pdfium;
mod baselines;
mod bench;
mod cache;
mod cli_args;
mod commands;
mod eval;
mod eval_checks;
mod manifest_gen;
mod ocr;
mod output;
mod parity;
mod process_util;
mod warm_bench;

pub(crate) use backend::*;
#[cfg(feature = "pdfium")]
pub(crate) use backend_pdfium::*;
pub(crate) use baselines::*;
pub(crate) use bench::*;
pub(crate) use cache::*;
pub(crate) use cli_args::*;
pub(crate) use commands::*;
pub(crate) use eval::*;
pub(crate) use eval_checks::*;
pub(crate) use manifest_gen::*;
pub(crate) use ocr::*;
pub(crate) use output::*;
pub(crate) use parity::*;
pub(crate) use process_util::*;
pub(crate) use warm_bench::*;

fn main() -> anyhow::Result<()> {
    cli_args::main_impl()
}
