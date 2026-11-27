use std::{fs, sync::Arc};

use gpui::{
    Context, Div, FontWeight, Image, ImageFormat, ObjectFit, div, img, prelude::*, px, rgb,
};
use serde::{Deserialize, Serialize};

use crate::{
    BORDER, Editor, MUTED, PANEL, Panel,
    architecture::{self, ArchitectureGraph, ConversionConfig},
    panel_button,
    templates::ascii_preview,
    text_files,
};

pub(super) fn environment_key() -> String {
    if std::env::var("ASCIIFLOW_AI_BASE_URL").is_ok_and(|endpoint| !endpoint.trim().is_empty()) {
        std::env::var("ASCIIFLOW_AI_API_KEY").unwrap_or_default()
    } else {
        String::new()
    }
}

#[derive(Deserialize, Serialize)]
pub(super) struct AiSettings {
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub strict_schema: bool,
}

impl AiSettings {
    pub fn load() -> Self {
        let stored: Option<Self> = crate::drawings::data_directory()
            .ok()
            .map(|dir| dir.join("ai-settings.json"))
            .and_then(|path| text_files::read_limited(&path, 16_384).ok())
            .and_then(|data| serde_json::from_slice(&data).ok());
        let mut settings = stored.unwrap_or(Self {
            base_url: "http://127.0.0.1:11434/v1".into(),
            model: String::new(),
            strict_schema: false,
        });
        if let Ok(base_url) = std::env::var("ASCIIFLOW_AI_BASE_URL") {
            settings.base_url = base_url;
        }
        if let Ok(model) = std::env::var("ASCIIFLOW_AI_MODEL") {
            settings.model = model;
        }
        settings
    }

    fn save(config: &ConversionConfig) -> Result<(), String> {
        config.endpoint()?;
        let directory = crate::drawings::data_directory().map_err(|error| error.to_string())?;
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let settings = Self {
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            strict_schema: config.strict_schema,
        };
        let data = serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?;
        text_files::write_bytes(&directory.join("ai-settings.json"), &data)
            .map_err(|error| error.to_string())
    }
}

pub(super) struct Review {
    source: String,
    drawing_name: String,
    model: String,
    graph: ArchitectureGraph,
    image: Option<Arc<Image>>,
    generation: u64,
    revision: u64,
}

impl Editor {
    pub(super) fn ai_config(&self, cx: &Context<Self>) -> ConversionConfig {
        let base_url = self.endpoint.read(cx).value().trim().to_owned();
        let api_key = if base_url == self.key_endpoint {
            self.api_key.read(cx).value().trim().to_owned()
        } else {
            String::new()
        };
        ConversionConfig {
            base_url,
            model: self.model.read(cx).value().trim().to_owned(),
            api_key,
            strict_schema: self.strict_schema,
        }
    }

    fn architecture_source(&self) -> Result<String, String> {
        let rect = self
            .active_region
            .and_then(|index| self.drawings.active().regions.get(index))
            .map(|region| region.rect)
            .or_else(|| self.canvas.selection())
            .ok_or("Select some cells or create a diagram region first.")?;
        let source = self.canvas.region_text(rect)?;
        if source.trim().is_empty() {
            return Err("The selected region is empty.".into());
        }
        if source.len() > architecture::MAX_SOURCE_BYTES {
            return Err("Select at most 32 KiB of diagram text.".into());
        }
        Ok(source)
    }

    pub(super) fn ai_panel(&self, cx: &mut Context<Self>) -> Div {
        let source = self.architecture_source();
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .child("Choose an OpenAI-compatible endpoint for architecture rendering."),
            )
            .child(div().child("Endpoint URL").child(self.endpoint.clone()))
            .child(div().child("Model").child(self.model.clone()))
            .child(panel_button(
                "ai-json-schema",
                if self.strict_schema {
                    "JSON Schema output: on"
                } else {
                    "JSON Schema output: off"
                },
                cx.listener(|editor, _, _, cx| {
                    editor.strict_schema = !editor.strict_schema;
                    cx.notify();
                }),
            ))
            .child(
                div()
                    .child("API key (optional, this session)")
                    .child(self.api_key.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_wrap()
                    .child(panel_button(
                        "remember-ai",
                        "Remember endpoint and model",
                        cx.listener(|editor, _, _, cx| {
                            let config = editor.ai_config(cx);
                            editor.message =
                                Some(AiSettings::save(&config).map_or_else(
                                    |error| error,
                                    |_| "Endpoint and model saved".into(),
                                ));
                            cx.notify();
                        }),
                    ))
                    .when(self.review.is_some(), |row| {
                        row.child(panel_button(
                            "review-last",
                            "Review last result",
                            cx.listener(|editor, _, _, cx| {
                                editor.panel = Some(Panel::Review);
                                cx.notify();
                            }),
                        ))
                    }),
            )
            .when(source.is_ok(), |panel| {
                panel.child(
                    div()
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .pt(px(12.0))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("ASCII sent to this endpoint"),
                        )
                        .child(
                            div()
                                .id("ai-source")
                                .max_h(px(195.0))
                                .overflow_scroll()
                                .bg(rgb(PANEL))
                                .child(ascii_preview(source.as_deref().unwrap_or_default())),
                        )
                        .child(panel_button(
                            "send-architecture",
                            if self.conversion_pending {
                                "Rendering…"
                            } else {
                                "Send and render architecture"
                            },
                            cx.listener(|editor, _, _, cx| editor.convert_architecture(cx)),
                        )),
                )
            })
            .when(source.is_err(), |panel| {
                panel.child(
                    div()
                        .text_color(rgb(MUTED))
                        .child(source.as_ref().err().cloned().unwrap_or_default()),
                )
            })
    }

    fn convert_architecture(&mut self, cx: &mut Context<Self>) {
        if self.conversion_pending {
            return;
        }
        let config = self.ai_config(cx);
        let source = match self
            .architecture_source()
            .and_then(|source| config.endpoint().map(|_| source))
        {
            Ok(source) => source,
            Err(error) => {
                self.message = Some(error);
                cx.notify();
                return;
            }
        };
        self.conversion_pending = true;
        self.review_generation = self.review_generation.wrapping_add(1);
        let generation = self.review_generation;
        let drawing_name = self.drawings.active().name.clone();
        let work = cx.background_executor().spawn(async move {
            let graph = architecture::convert_ascii(&config, &source)?;
            let image = preview_image(&graph)?;
            Ok::<_, String>(Review {
                source,
                drawing_name,
                model: config.model,
                graph,
                image: Some(image),
                generation,
                revision: 0,
            })
        });
        cx.spawn(async move |editor, cx| {
            let result = work.await;
            let _ = editor.update(cx, |editor, cx| {
                editor.conversion_pending = false;
                match result {
                    Ok(review) => {
                        editor.review = Some(review);
                        if editor.panel == Some(Panel::Ai) {
                            editor.panel = Some(Panel::Review);
                        }
                        editor.message = Some("Architecture ready for review".into());
                    }
                    Err(error) => editor.message = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn review_panel(&self, cx: &mut Context<Self>) -> Div {
        let Some(review) = &self.review else {
            return div().child("Render a diagram to begin reviewing it.");
        };
        let panels = div()
            .flex()
            .gap(px(10.0))
            .child(
                div()
                    .id("architecture-source")
                    .flex_1()
                    .min_w_0()
                    .h(px(330.0))
                    .overflow_scroll()
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(ascii_preview(&review.source)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(330.0))
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .when_some(review.image.clone(), |panel, image| {
                        panel.child(img(image).size_full().object_fit(ObjectFit::Contain))
                    }),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(div().text_color(rgb(MUTED)).child(
                "Check the inferred groups and connections against the source before exporting.",
            ))
            .child(panels)
            .child(div().text_color(rgb(MUTED)).child(format!(
                "Source drawing: {} · Requested model: {}",
                review.drawing_name, review.model
            )))
            .children(
                review
                    .graph
                    .warnings
                    .iter()
                    .map(|warning| div().text_color(rgb(0x8a4915)).child(warning.clone())),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(panel_button(
                        "architecture-html",
                        "Export HTML",
                        cx.listener(|editor, _, _, cx| editor.export_architecture(true, cx)),
                    ))
                    .child(panel_button(
                        "architecture-svg",
                        "Export SVG",
                        cx.listener(|editor, _, _, cx| editor.export_architecture(false, cx)),
                    )),
            )
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Components"))
            .children(review.graph.nodes.iter().enumerate().map(|(index, node)| {
                let group = node
                    .group
                    .as_ref()
                    .and_then(|id| review.graph.groups.iter().find(|group| group.id == *id))
                    .map_or("No group", |group| group.label.as_str());
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(div().truncate().child(format!("{} · {group}", node.label)))
                    .when(!review.graph.groups.is_empty(), |row| {
                        row.child(panel_button(
                            ("change-group", index as u64),
                            "Change group",
                            cx.listener(move |editor, _, _, cx| {
                                editor.edit_graph(cx, |graph| {
                                    let next = graph.nodes[index]
                                        .group
                                        .as_ref()
                                        .and_then(|id| {
                                            graph.groups.iter().position(|group| group.id == *id)
                                        })
                                        .map_or(0, |index| index + 1);
                                    graph.nodes[index].group =
                                        graph.groups.get(next).map(|group| group.id.clone());
                                });
                            }),
                        ))
                    })
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Connections"))
                    .when(
                        review.graph.nodes.len() > 1 && review.graph.edges.len() < 160,
                        |row| {
                            row.child(panel_button(
                                "add-connection",
                                "Add connection",
                                cx.listener(|editor, _, _, cx| {
                                    editor.edit_graph(cx, |graph| {
                                        graph.edges.push(architecture::Edge {
                                            from: graph.nodes[0].id.clone(),
                                            to: graph.nodes[1].id.clone(),
                                            label: None,
                                        })
                                    });
                                }),
                            ))
                        },
                    ),
            )
            .children(review.graph.edges.iter().enumerate().map(|(index, edge)| {
                let label = |id: &str| {
                    review
                        .graph
                        .nodes
                        .iter()
                        .find(|node| node.id == id)
                        .map(|node| node.label.clone())
                        .unwrap_or_else(|| id.into())
                };
                let caption = format!(
                    "{} → {}{}",
                    label(&edge.from),
                    label(&edge.to),
                    edge.label
                        .as_ref()
                        .map_or(String::new(), |label| format!(" · {label}"))
                );
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().flex_1().truncate().child(caption))
                    .child(panel_button(
                        ("from-edge", index as u64),
                        "From",
                        cx.listener(move |editor, _, _, cx| editor.shift_edge(index, true, cx)),
                    ))
                    .child(panel_button(
                        ("to-edge", index as u64),
                        "To",
                        cx.listener(move |editor, _, _, cx| editor.shift_edge(index, false, cx)),
                    ))
                    .child(panel_button(
                        ("reverse-edge", index as u64),
                        "Reverse",
                        cx.listener(move |editor, _, _, cx| {
                            editor.edit_graph(cx, |graph| {
                                let edge = &mut graph.edges[index];
                                std::mem::swap(&mut edge.from, &mut edge.to);
                            })
                        }),
                    ))
                    .child(panel_button(
                        ("remove-edge", index as u64),
                        "Remove",
                        cx.listener(move |editor, _, _, cx| {
                            editor.edit_graph(cx, |graph| {
                                graph.edges.remove(index);
                            })
                        }),
                    ))
            }))
    }

    fn shift_edge(&mut self, index: usize, from: bool, cx: &mut Context<Self>) {
        self.edit_graph(cx, |graph| {
            let edge = &mut graph.edges[index];
            let (changed, fixed) = if from {
                (&mut edge.from, &edge.to)
            } else {
                (&mut edge.to, &edge.from)
            };
            if let Some(current) = graph.nodes.iter().position(|node| node.id == *changed)
                && let Some(next) = graph
                    .nodes
                    .iter()
                    .cycle()
                    .skip(current + 1)
                    .take(graph.nodes.len())
                    .find(|node| node.id != *fixed)
            {
                changed.clone_from(&next.id);
            }
        });
    }

    fn edit_graph(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut ArchitectureGraph)) {
        let Some(review) = &mut self.review else {
            return;
        };
        if review.image.is_none() {
            self.message = Some("Wait for the current preview to finish.".into());
            cx.notify();
            return;
        }
        let mut graph = review.graph.clone();
        edit(&mut graph);
        if let Err(error) = graph.validate() {
            self.message = Some(error);
            cx.notify();
            return;
        }
        review.revision += 1;
        let revision = review.revision;
        let generation = review.generation;
        let work = cx
            .background_executor()
            .spawn(async move { preview_image(&graph).map(|image| (graph, image)) });
        let previous = review.image.take();
        cx.spawn(async move |editor, cx| {
            let result = work.await;
            let _ = editor.update(cx, |editor, cx| {
                if let Some(review) = &mut editor.review
                    && review.generation == generation
                    && review.revision == revision
                {
                    match result {
                        Ok((graph, image)) => {
                            review.graph = graph;
                            review.image = Some(image);
                        }
                        Err(error) => {
                            review.image = previous;
                            editor.message = Some(error);
                        }
                    }
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn export_architecture(&mut self, html: bool, cx: &mut Context<Self>) {
        let Some(review) = &self.review else {
            return;
        };
        if review.image.is_none() {
            self.message = Some("Wait for the updated preview before exporting.".into());
            cx.notify();
            return;
        }
        let graph = review.graph.clone();
        self.export_generated(
            if html {
                "architecture.html"
            } else {
                "architecture.svg"
            },
            move || {
                if html {
                    architecture::render_html(&graph)
                } else {
                    architecture::render_svg(&graph)
                }
                .map(String::into_bytes)
            },
            cx,
        );
    }
}

fn preview_image(graph: &ArchitectureGraph) -> Result<Arc<Image>, String> {
    use resvg::{tiny_skia, usvg};
    use std::sync::OnceLock;

    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let fontdb = FONTS.get_or_init(|| {
        let mut fonts = usvg::fontdb::Database::new();
        fonts.load_system_fonts();
        Arc::new(fonts)
    });
    let markup = architecture::render_svg(graph)?;
    let tree = usvg::Tree::from_str(
        &markup,
        &usvg::Options {
            fontdb: fontdb.clone(),
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    let size = tree.size();
    let scale = (1_800.0 / size.width())
        .min(1_200.0 / size.height())
        .min(1.0);
    let mut pixmap = tiny_skia::Pixmap::new(
        (size.width() * scale).ceil() as u32,
        (size.height() * scale).ceil() as u32,
    )
    .ok_or("Could not allocate the architecture preview.")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let png = pixmap.encode_png().map_err(|error| error.to_string())?;
    Ok(Arc::new(Image::from_bytes(ImageFormat::Png, png)))
}
