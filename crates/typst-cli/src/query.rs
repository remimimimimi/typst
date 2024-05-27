use std::path::PathBuf;

use comemo::{Track, Validate};
// use comemo::{Tracked, Validate};
use ecow::{eco_format, EcoString};
use serde::Serialize;
use typst::diag::{bail, StrResult};
use typst::engine::{Engine, Route};
use typst::eval::{eval_string, EvalMode, Tracer};
use typst::foundations::{
    Content, IntoValue, LocatableSelector, Scope, StyleChain, Styles,
};
use typst::introspection::{Introspector, Locator};
use typst::layout::LayoutRoot;
use typst::model::Document;
use typst::syntax::Span;
use typst::World;

use crate::args::{QueryCommand, SerializationFormat};
use crate::compile::print_diagnostics;
use crate::set_failed;
use crate::world::SystemWorld;

/// Execute a query command.
pub fn query(command: &QueryCommand) -> StrResult<()> {
    let mut world = SystemWorld::new(&command.common)?;

    // Reset everything and ensure that the main file is present.
    world.reset();

    world.source(world.main()).map_err(|err| err.to_string())?;

    let mut tracer = Tracer::new();
    let result = typst::compile(&world, &mut tracer);
    // let warnings = tracer.warnings();

    let styles = tracer.values().first().unwrap().1.clone().unwrap();

    match result {
        // Retrieve and print query results.
        Ok(document) => {
            let data: Vec<Content> = retrieve(&world, command, &document)?;
            // let serialized = format(data, command)?;

            let first_match = data.first().unwrap();
            let world_dyn: &dyn World = &world;
            let trackable_world = world_dyn.track();
            let constraint = <Introspector as Validate>::Constraint::new();
            let mut tracer = Tracer::new();
            let mut locator = Locator::new();
            let mut engine = Engine {
                world: trackable_world,
                route: Route::default(),
                tracer: tracer.track_mut(),
                locator: &mut locator,
                introspector: document.introspector.track_with(&constraint), // &world.main(),
            };

            let new_doc = first_match
                .layout_root(&mut engine, StyleChain::new(&styles))
                .unwrap();

            // tracer.inspect(first_match.span());

            let first_frame = &new_doc.pages.first().unwrap().frame;
            let output_path = PathBuf::from("./output.svg");
            let svg = typst_svg::svg(first_frame);
            std::fs::write(output_path, svg).unwrap();

            // println!("{serialized}");
            // print_diagnostics(&world, &[], &warnings, command.common.diagnostic_format)
            //     .map_err(|err| eco_format!("failed to print diagnostics ({err})"))?;
        }

        // Print diagnostics.
        Err(errors) => {
            set_failed();
            // print_diagnostics(
            //     &world,
            //     &errors,
            //     &warnings,
            //     command.common.diagnostic_format,
            // )
            // .map_err(|err| eco_format!("failed to print diagnostics ({err})"))?;
        }
    }

    Ok(())
}

/// Retrieve the matches for the selector.
fn retrieve(
    world: &dyn World,
    command: &QueryCommand,
    document: &Document,
) -> StrResult<Vec<Content>> {
    let selector = eval_string(
        world.track(),
        &command.selector,
        Span::detached(),
        EvalMode::Code,
        Scope::default(),
    )
    .map_err(|errors| {
        let mut message = EcoString::from("failed to evaluate selector");
        for (i, error) in errors.into_iter().enumerate() {
            message.push_str(if i == 0 { ": " } else { ", " });
            message.push_str(&error.message);
        }
        message
    })?
    .cast::<LocatableSelector>()?;

    Ok(document
        .introspector
        .query(&selector.0)
        .into_iter()
        .collect::<Vec<_>>())
}

/// Format the query result in the output format.
fn format(elements: Vec<Content>, command: &QueryCommand) -> StrResult<String> {
    if command.one && elements.len() != 1 {
        bail!("expected exactly one element, found {}", elements.len());
    }

    let mapped: Vec<_> = elements
        .into_iter()
        .filter_map(|c| match &command.field {
            Some(field) => dbg!(c).get_by_name(field),
            _ => Some(dbg!(c).into_value()),
        })
        .collect();

    if command.one {
        let Some(value) = mapped.first() else {
            bail!("no such field found for element");
        };
        serialize(value, command.format, command.pretty)
    } else {
        serialize(&mapped, command.format, command.pretty)
    }
}

/// Serialize data to the output format.
fn serialize(
    data: &impl Serialize,
    format: SerializationFormat,
    pretty: bool,
) -> StrResult<String> {
    match format {
        SerializationFormat::Json => {
            if pretty {
                serde_json::to_string_pretty(data).map_err(|e| eco_format!("{e}"))
            } else {
                serde_json::to_string(data).map_err(|e| eco_format!("{e}"))
            }
        }
        SerializationFormat::Yaml => {
            serde_yaml::to_string(data).map_err(|e| eco_format!("{e}"))
        }
    }
}
