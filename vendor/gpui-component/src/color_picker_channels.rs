//! Meshcraft: HSL/RGB channel controls and a shared alpha bar for all picker modes.
use super::*;
use crate::slider::{Slider, SliderEvent, SliderState};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Hsl,
    Rgb,
    Palette,
}
impl ColorPickerState {
    fn pick_plane(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.plane_bounds;
        let x = (f32::from(position.x - bounds.origin.x) / f32::from(bounds.size.width).max(1.))
            .clamp(0., 1.);
        let y = (f32::from(position.y - bounds.origin.y) / f32::from(bounds.size.height).max(1.))
            .clamp(0., 1.);
        let c = self.value.unwrap_or(Hsla {
            h: 0.,
            s: 0.,
            l: 0.5,
            a: 1.,
        });
        let value = if self.mode == Mode::Hsl {
            Hsla {
                s: x,
                l: 1. - y,
                ..c
            }
        } else {
            let rgba: gpui::Rgba = c.into();
            gpui::Rgba {
                r: x,
                g: 1. - y,
                ..rgba
            }
            .into()
        };
        self.update_value(Some(value), true, window, cx);
    }
    pub(super) fn channels(&self) -> [f32; 4] {
        let c = self.value.unwrap_or(Hsla {
            h: 0.,
            s: 0.,
            l: 0.5,
            a: 1.,
        });
        if self.mode == Mode::Rgb {
            let r: gpui::Rgba = c.into();
            [r.r, r.g, r.b, r.a]
        } else {
            [c.h, c.s, c.l, c.a]
        }
    }
    pub(super) fn sync_channels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let c = self.channels();
        for (i, v) in c.into_iter().enumerate() {
            self.sliders[i].update(cx, |s, cx| s.set_value(v, window, cx));
            let scale = if i == 3 || self.mode != Mode::Rgb && i > 0 {
                100.
            } else if self.mode == Mode::Rgb {
                255.
            } else {
                360.
            };
            self.channels[i].update(cx, |s, cx| {
                s.set_value(format!("{:.1}", v * scale), window, cx)
            });
        }
    }
    pub(super) fn channel(
        &mut self,
        index: usize,
        v: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !v.is_finite() || !(0.0..=1.).contains(&v) {
            self.channel_error = Some("Channel value is outside its range".into());
            cx.notify();
            return;
        }
        let mut c = self.channels();
        c[index] = v;
        let value = if self.mode == Mode::Rgb {
            Hsla::from(gpui::Rgba {
                r: c[0],
                g: c[1],
                b: c[2],
                a: c[3],
            })
        } else {
            Hsla {
                h: c[0],
                s: c[1],
                l: c[2],
                a: c[3],
            }
        };
        self.channel_error = None;
        self.update_value(Some(value), true, window, cx);
    }
}
pub(super) fn initialize(
    window: &mut Window,
    cx: &mut Context<ColorPickerState>,
) -> (
    [Entity<InputState>; 4],
    [Entity<SliderState>; 4],
    Vec<Subscription>,
) {
    let inputs = std::array::from_fn(|_| cx.new(|cx| InputState::new(window, cx)));
    let sliders =
        std::array::from_fn(|_| cx.new(|_| SliderState::new().min(0.).max(1.).step(0.001)));
    let mut subscriptions = vec![];
    for i in 0..4 {
        subscriptions.push(cx.subscribe_in(
            &sliders[i],
            window,
            move |this, _, event: &SliderEvent, window, cx| {
                let SliderEvent::Change(v) = event;
                this.channel(i, v.end(), window, cx);
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &inputs[i],
            window,
            move |this, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    let scale = if i == 3 || this.mode != Mode::Rgb && i > 0 {
                        100.
                    } else if this.mode == Mode::Rgb {
                        255.
                    } else {
                        360.
                    };
                    if let Ok(v) = input.read(cx).value().parse::<f32>() {
                        this.channel(i, v / scale, window, cx);
                    } else {
                        this.channel_error = Some("Enter a numeric channel value".into());
                        cx.notify();
                    }
                }
            },
        ));
    }
    (inputs, sliders, subscriptions)
}
impl ColorPicker {
    pub(super) fn mode_controls(&self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state.clone();
        let mode = state.read(cx).mode;
        h_flex().gap_1().children(
            [
                (Mode::Hsl, "HSL"),
                (Mode::Rgb, "RGB"),
                (Mode::Palette, "Swatches"),
            ]
            .into_iter()
            .map(|(m, label)| {
                let state = state.clone();
                Button::new(label)
                    .small()
                    .label(label)
                    .selected(mode == m)
                    .on_click(window.listener_for(&state, move |this, _, window, cx| {
                        this.mode = m;
                        this.sync_channels(window, cx);
                        cx.notify();
                    }))
            }),
        )
    }
    pub(super) fn channel_controls(&self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state.clone();
        let mode = state.read(cx).mode;
        let value = state.read(cx).value.unwrap_or(Hsla {
            h: 0.,
            s: 0.,
            l: 0.5,
            a: 1.,
        });
        let rgba: gpui::Rgba = value.into();
        let labels = if mode == Mode::Rgb {
            ["R", "G", "B", "Opacity %"]
        } else {
            ["Hue °", "Sat %", "Light %", "Opacity %"]
        };
        let mut body = v_flex().gap_2();
        if mode != Mode::Palette {
            let gradient = |from: Hsla, to: Hsla| {
                gpui::linear_gradient(
                    180.,
                    gpui::linear_color_stop(from, 0.),
                    gpui::linear_color_stop(to, 1.),
                )
            };
            let mut strips = h_flex().h(px(120.)).w_full();
            for x in 0..128 {
                let ratio = x as f32 / 127.;
                let middle = Hsla {
                    h: value.h,
                    s: ratio,
                    l: 0.5,
                    a: value.a,
                };
                let strip = if mode == Mode::Hsl {
                    v_flex()
                        .h_full()
                        .flex_1()
                        .child(
                            div()
                                .flex_1()
                                .w_full()
                                .bg(gradient(Hsla { l: 1., ..middle }, middle)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .w_full()
                                .bg(gradient(middle, Hsla { l: 0., ..middle })),
                        )
                        .into_any_element()
                } else {
                    div()
                        .flex_1()
                        .h_full()
                        .bg(gradient(
                            gpui::Rgba {
                                r: ratio,
                                g: 1.,
                                b: rgba.b,
                                a: rgba.a,
                            }
                            .into(),
                            gpui::Rgba {
                                r: ratio,
                                g: 0.,
                                b: rgba.b,
                                a: rgba.a,
                            }
                            .into(),
                        ))
                        .into_any_element()
                };
                strips = strips.child(strip);
            }
            let measure = state.clone();
            let plane = div()
                .id("color-plane")
                .relative()
                .w_full()
                .h(px(120.))
                .child(strips)
                .child(
                    canvas(
                        move |bounds, _, cx| measure.update(cx, |s, _| s.plane_bounds = bounds),
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    window.listener_for(
                        &state,
                        |this, event: &gpui::MouseDownEvent, window, cx| {
                            this.plane_drag = true;
                            this.pick_plane(event.position, window, cx);
                            cx.stop_propagation();
                        },
                    ),
                )
                .on_mouse_move(window.listener_for(
                    &state,
                    |this, event: &gpui::MouseMoveEvent, window, cx| {
                        if this.plane_drag && event.pressed_button == Some(MouseButton::Left) {
                            this.pick_plane(event.position, window, cx);
                        }
                    },
                ))
                .on_mouse_up(
                    MouseButton::Left,
                    window.listener_for(&state, |this, _, _, _| this.plane_drag = false),
                );
            body = body.child(plane);
        }

        for i in if mode == Mode::Palette { 3..4 } else { 0..4 } {
            let input = state.read(cx).channels[i].clone();
            let slider = state.read(cx).sliders[i].clone();
            // Alpha is composited over alternating tiles so transparency remains visible.
            let gradient = h_flex().w_full().h(px(7.)).children((0..32).map(|x| {
                let t = x as f32 / 31.;
                let mut c = value;
                if i == 3 {
                    c.a = t;
                } else if mode == Mode::Hsl {
                    match i {
                        0 => {
                            c.h = t;
                            c.s = 1.;
                            c.l = 0.5;
                        }
                        1 => c.s = t,
                        _ => c.l = t,
                    }
                } else {
                    let mut r = rgba;
                    match i {
                        0 => r.r = t,
                        1 => r.g = t,
                        _ => r.b = t,
                    };
                    c = r.into();
                }
                let mut r: gpui::Rgba = c.into();
                let bg = if x % 2 == 0 { 0.25 } else { 0.75 };
                r.r = r.r * r.a + bg * (1. - r.a);
                r.g = r.g * r.a + bg * (1. - r.a);
                r.b = r.b * r.a + bg * (1. - r.a);
                r.a = 1.;
                div().flex_1().h_full().bg(r)
            }));
            body = body.child(
                h_flex()
                    .gap_2()
                    .child(div().w(px(62.)).text_xs().child(labels[i]))
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_0()
                            .child(gradient)
                            .child(Slider::new(&slider)),
                    )
                    .child(div().w(px(72.)).child(Input::new(&input).small())),
            );
        }
        body.when_some(state.read(cx).channel_error.clone(), |v, e| {
            v.child(div().text_xs().child(e))
        })
    }
}
