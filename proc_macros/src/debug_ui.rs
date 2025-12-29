use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    parse_macro_input, punctuated::Punctuated, token::Comma, Attribute, Data, DeriveInput, Field,
    Fields, LitStr, Type, TypePath,
};

// -------------------------------------------------------
// `#[derive(DrawDebugUi)]` PROCEDURAL MACRO:
// -------------------------------------------------------

/*
| Attribute                   | Effect                    |
| --------------------------- | ------------------------- |
| #[debug_ui(skip)]           | Skip the field entirely   |
| #[debug_ui(separator)]      | Add ui.separator() after  |
| #[debug_ui(nested)]         | Nested struct with own fn |
| #[debug_ui(label = "...")]  | Override default label    |
| #[debug_ui(format = "...")] | Use custom format string  |
| #[debug_ui(edit = "...")]   | Use imgui edit widget     |

| Only meaningful when used together with `edit`:
| ----------------------------------- | -------------------------------- |
| #[debug_ui(step = "...")]           | Edit widget step (int, float)    |
| #[debug_ui(min = "...")]            | Edit widget min (int, float)     |
| #[debug_ui(max = "...")]            | Edit widget max (int, float)     |
| #[debug_ui(display_format = "...")] | Edit widget display_format       |
| #[debug_ui(widget = "...")]         | Edit widget kind (e.g. "slider") |
*/

#[derive(Default)]
struct DebugUiAttrs {
    skip: bool,
    separator: bool,
    nested: bool,
    label: Option<String>,
    format: Option<String>,
    edit: Option<String>,
    step: Option<String>,
    min: Option<String>,
    max: Option<String>,
    display_format: Option<String>,
    widget: Option<String>,
}

fn parse_debug_ui_attrs(attrs: &[Attribute]) -> DebugUiAttrs {
    let mut result = DebugUiAttrs::default();

    for attr in attrs {
        if !attr.path().is_ident("debug_ui") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                result.skip = true;
            } else if meta.path.is_ident("separator") {
                result.separator = true;
            } else if meta.path.is_ident("nested") {
                result.nested = true;
            } else if meta.path.is_ident("label") {
                let value = meta.value()?;
                let label: LitStr = value.parse()?;
                result.label = Some(label.value());
            } else if meta.path.is_ident("format") {
                let value = meta.value()?;
                let format: LitStr = value.parse()?;
                result.format = Some(format.value());
            } else if meta.path.is_ident("step") {
                let value = meta.value()?;
                let step: LitStr = value.parse()?;
                result.step = Some(step.value());
            } else if meta.path.is_ident("min") {
                let value = meta.value()?;
                let min: LitStr = value.parse()?;
                result.min = Some(min.value());
            } else if meta.path.is_ident("max") {
                let value = meta.value()?;
                let max: LitStr = value.parse()?;
                result.max = Some(max.value());
            } else if meta.path.is_ident("display_format") {
                let value = meta.value()?;
                let display_format: LitStr = value.parse()?;
                result.display_format = Some(display_format.value());
            } else if meta.path.is_ident("widget") {
                let value = meta.value()?;
                let widget: LitStr = value.parse()?;
                result.widget = Some(widget.value());
            } else if meta.path.is_ident("edit") {
                // `= "value"` part is optional.
                result.edit = Some({
                    if let Ok(value) = meta.value() {
                        if let Ok(parsed) = value.parse::<LitStr>() {
                            parsed.value()
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    }
                });
            } else {
                return Err(meta.error(format!("Unsupported attribute: '{}'",
                    meta.path.get_ident().map_or_else(|| "unknown".to_string(), |id| id.to_string()))));
            }
            Ok(())
        }).expect("Invalid meta attributes!");
    }

    result
}

#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Eq)]
enum FieldKind {
    Bool,
    Int,
    Float,
    String,
    Unknown,
}

fn infer_field_kind(field: &Field) -> FieldKind {
    if let Type::Path(TypePath { path, .. }) = &field.ty {
        if let Some(ident) = path.get_ident() {
            let kind = match ident.to_string().as_str() {
                "bool" => FieldKind::Bool,
                "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize" => {
                    FieldKind::Int
                }
                "f32" | "f64" | "Seconds" => FieldKind::Float,
                "String" => FieldKind::String,
                _ => FieldKind::Unknown,
            };
            return kind;
        }
    }
    FieldKind::Unknown
}

fn calc_field_name_padding(fields: &Punctuated<Field, Comma>) -> usize {
    let mut longest_field_name = 0;
    for field in fields.iter() {
        let attrs = parse_debug_ui_attrs(&field.attrs);
        if attrs.skip || attrs.edit.is_some() {
            // edit fields use imgui input widgets.
            continue;
        }

        let label_str = attrs.label.unwrap_or_else(|| field.ident.as_ref().unwrap().to_string());

        longest_field_name = longest_field_name.max(label_str.len());
    }
    longest_field_name + 1
}

fn has_any_edit_attr(fields: &Punctuated<Field, Comma>) -> bool {
    for field in fields.iter() {
        let attrs = parse_debug_ui_attrs(&field.attrs);
        if attrs.edit.is_some() {
            return true;
        }
    }
    false
}

fn snake_case_to_title(s: &str) -> String {
    s.split('_')
     .map(|word| {
         let mut chars = word.chars();
         match chars.next() {
             Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
             None => String::new(),
         }
     })
     .collect::<Vec<_>>()
     .join(" ")
}

// Entry point function.
pub fn draw_debug_ui_proc_macro_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = input.ident;

    let fields = if let Data::Struct(data_struct) = input.data {
        match data_struct.fields {
            Fields::Named(named_fields) => named_fields.named,
            _ => {
                return syn::Error::new_spanned(struct_name, "Expected named fields!")
                    .to_compile_error()
                    .into();
            }
        }
    } else {
        return syn::Error::new_spanned(struct_name, "Only structs are supported!")
            .to_compile_error()
            .into();
    };

    let field_name_padding = calc_field_name_padding(&fields);
    let has_any_edit_attr = has_any_edit_attr(&fields);

    let field_lines = fields.iter().map(|field| {
        let attrs = parse_debug_ui_attrs(&field.attrs);
        if attrs.skip {
            // Ignore this field.
            return None;
        }

        let field_name = &field.ident;
        let field_kind = infer_field_kind(field);

        let label_str = attrs.label.unwrap_or_else(|| {
            let name_str = field_name.as_ref().unwrap().to_string();
            snake_case_to_title(&name_str)
        });

        let format_str = attrs.format.unwrap_or_else(|| {
            let value_format_specifier = if field_kind == FieldKind::Float {
                "{:.2}" // Use 2 decimal digits only for float variables.
            } else {
                "{}" // Default Display format.
            };
            format!("{:<width$}: {}", label_str, value_format_specifier, width = field_name_padding)
        });

        let format_str_lit = LitStr::new(&format_str, Span::call_site());

        let mut tokens = TokenStream::new();

        // Use imgui edit widget?
        if let Some(edit) = attrs.edit {
            let read_only = {
                if edit.is_empty() {
                    false
                } else if edit == "readonly" {
                    true
                } else {
                    panic!("Invalid value '{edit}' for 'edit' attribute! Expected 'readonly' or empty.")
                }
            };

            let (slider, button) = {
                if let Some(widget) = attrs.widget {
                    if widget == "slider" {
                        (true, false)
                    } else if widget == "button" {
                        if field_kind != FieldKind::Bool {
                            panic!("Button widget only works with bool fields!");
                        }
                        (false, true)
                    } else {
                        panic!("Invalid value '{widget}' for 'widget' attribute! Expected 'slider' or 'button'.")
                    }
                } else {
                    (false, false)
                }
            };

            let field_tokens = match field_kind {
                FieldKind::Bool => {
                    if read_only {
                        if button {
                            quote! { ui.button(#label_str); }
                        } else {
                            quote! {
                                {
                                    // Write into a local variable so any change will be discarded.
                                    let mut b_curr_value_ = self.#field_name;
                                    ui.checkbox(#label_str, &mut b_curr_value_);
                                }
                            }
                        }
                    } else if button {
                        quote! { self.#field_name = ui.button(#label_str); }
                    } else {
                        quote! { ui.checkbox(#label_str, &mut self.#field_name); }
                    }
                },
                FieldKind::Int => {
                    let display_format = attrs.display_format.unwrap_or("%i".into());
                    if slider {
                        let min: i32 = attrs.min.unwrap_or("0".into()).parse().expect("Invalid 'min' attribute!");
                        let max: i32 = attrs.max.unwrap_or("999".into()).parse().expect("Invalid 'max' attribute!");
                        if read_only {
                            quote! {
                                {
                                    // Write into a local variable so any change will be discarded.
                                    let mut i_curr_value_ = self.#field_name;
                                    ui.slider_config(#label_str, #min, #max)
                                        .display_format(#display_format)
                                        .build(&mut i_curr_value_);
                                }
                            }
                        } else {
                            quote! {
                                ui.slider_config(#label_str, #min, #max)
                                    .display_format(#display_format)
                                    .build(&mut self.#field_name);
                            }
                        }
                    } else {
                        let step: i32 = attrs.step.unwrap_or("1".into()).parse().expect("Invalid 'step' attribute!");
                        quote! {
                            ui.input_int(#label_str, &mut self.#field_name)
                                .display_format(#display_format)
                                .read_only(#read_only)
                                .step(#step)
                                .build();
                        }
                    }
                },
                FieldKind::Float => {
                   let display_format = attrs.display_format.unwrap_or("%.2f".into());
                    if slider {
                        let min: f32 = attrs.min.unwrap_or("0.0".into()).parse().expect("Invalid 'min' attribute!");
                        let max: f32 = attrs.max.unwrap_or("999.0".into()).parse().expect("Invalid 'max' attribute!");
                        if read_only {
                            quote! {
                                {
                                    // Write into a local variable so any change will be discarded.
                                    let mut f_curr_value_ = self.#field_name;
                                    ui.slider_config(#label_str, #min, #max)
                                        .display_format(#display_format)
                                        .build(&mut f_curr_value_);
                                }
                            }
                        } else {
                            quote! {
                                ui.slider_config(#label_str, #min, #max)
                                    .display_format(#display_format)
                                    .build(&mut self.#field_name);
                            }
                        }
                    } else {
                        let step: f32 = attrs.step.unwrap_or("1.0".into()).parse().expect("Invalid 'step' attribute!");
                        quote! {
                            ui.input_float(#label_str, &mut self.#field_name)
                                .display_format(#display_format)
                                .read_only(#read_only)
                                .step(#step)
                                .build();
                        }
                    }
                },
                FieldKind::String => {
                    quote! {
                        ui.input_text(#label_str, &mut self.#field_name)
                            .read_only(#read_only)
                            .build();
                    }
                },
                FieldKind::Unknown => {
                    if attrs.nested {
                        // Nested structure with its own draw_debug_ui method.
                        quote! { self.#field_name.draw_debug_ui_with_header(stringify!(#field_name), ui_sys); }
                    } else {
                        // Fallback: Try format Display text.
                        quote! { ui.text(format!(#format_str_lit, self.#field_name)); }
                    }
                },
            };

            tokens.extend(field_tokens);
        } else if attrs.nested {
            // Nested structure with its own draw_debug_ui method.
            tokens.extend(quote! {
                self.#field_name.draw_debug_ui_with_header(stringify!(#field_name), ui_sys);
            });
        } else {
            // Read only text field.
            tokens.extend(quote! {
                ui.text(format!(#format_str_lit, self.#field_name));
            });
        }

        if attrs.separator {
            tokens.extend(quote! {
                ui.separator();
            });
        }

        tokens.into()
    });

    // If we have edit widgets we need mut self, else just self is fine.
    let self_argument = {
        if has_any_edit_attr {
            quote! { &mut self }
        } else {
            quote! { &self }
        }
    };

    // Add all generics, lifetimes, etc to the impl block.
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let output = quote! {
        impl #impl_generics #struct_name #ty_generics #where_clause {
            pub fn draw_debug_ui(#self_argument, ui_sys: &crate::ui::UiSystem) {
                let ui = ui_sys.ui();
                #(#field_lines)*
            }
            pub fn draw_debug_ui_with_header(#self_argument, header: &str, ui_sys: &crate::ui::UiSystem) {
                let ui = ui_sys.ui();
                if ui.collapsing_header(header, imgui::TreeNodeFlags::empty()) {
                    self.draw_debug_ui(ui_sys);
                }
            }
        }
    };

    output.into()
}
