//! Viewport settings, project controls and the in-app guide.
use super::*;
use iced::widget::column;

impl App {
    pub(super) fn settings_view(&self) -> Element<'_, Message> {
        let s = &self.project.settings;
        let mut controls = column![
            text("Viewport").size(18),
            text("Axis limits accept expressions. Zoom stays centred under the cursor.")
                .size(11)
                .color(MUTED),
            row![
                self.axis_field(0, "x min / Re min"),
                self.axis_field(1, "x max / Re max")
            ]
            .spacing(12),
            row![
                self.axis_field(2, "y min / Im min"),
                self.axis_field(3, "y max / Im max")
            ]
            .spacing(12),
            self.setting(
                Field::GridStep,
                "X grid spacing (auto or positive; log: decades)",
                &s.grid_step
            ),
            button(text("Grid, axes and background settings").size(12))
                .style(button::secondary)
                .on_press(Message::Appearance(appearance::Action::Open(
                    appearance::Panel::Guides
                )))
        ]
        .spacing(13);
        controls = controls
            .push(text("Appearance").size(18).boxed())
            .push(
                text(format!("Output opacity · {:.0}%", s.line_opacity * 100.))
                    .size(12)
                    .boxed(),
            )
            .push(
                slider(0.0..=1.0, s.line_opacity, Message::LineOpacity)
                    .step(0.01)
                    .boxed(),
            );
        if s.coloring_enabled() {
            let legend = concat!(
                "Colour and contour settings are in the toolbar colour gear. ",
                "Colouring runs in background workers; curve edits stay live."
            );
            controls = controls
                .push(
                    text(format!(
                        "Domain colouring opacity · {:.0}%",
                        s.color_opacity * 100.
                    ))
                    .size(12)
                    .boxed(),
                )
                .push(
                    slider(0.0..=1.0, s.color_opacity, Message::ColorOpacity)
                        .step(0.01)
                        .boxed(),
                )
                .push(text("Colour domain").size(18).boxed())
                .push(
                    text("Follows the current viewport, plus a margin on each side.")
                        .size(11)
                        .color(MUTED)
                        .boxed(),
                )
                .push(
                    pick_list(
                        Some(s.domain_extent),
                        [Extent::Percent, Extent::Absolute],
                        ToString::to_string,
                    )
                    .on_select(Message::Extent)
                    .boxed(),
                )
                .push(
                    row![
                        self.setting(Field::MarginX, "Horizontal margin", &s.domain_margin[0]),
                        self.setting(Field::MarginY, "Vertical margin", &s.domain_margin[1]),
                    ]
                    .spacing(12)
                    .boxed(),
                )
                .push(self.resolution_control())
                .push(text(legend).size(11).color(MUTED).boxed());
        }
        controls
            .push(
                button(text("Reset viewport").size(12))
                    .style(button::secondary)
                    .on_press(Message::ResetView)
                    .boxed(),
            )
            .push(space().height(16).boxed())
            .boxed()
    }
    pub(super) fn help_view(&self) -> Element<'_, Message> {
        let intro = concat!(
            "Edit any expression or drag a marker: the function updates automatically. ",
            "There is no run button. Invalid edits pause the output, retain valid handles, and mark the affected fields."
        );
        let sections = [
            (
                "Dragging",
                concat!(
                    "All handles move independently in 2D. In real mode a handle's x coordinate ",
                    "supplies its scalar parameter; y only arranges the handle. In complex mode ",
                    "x and y supply the real and imaginary parts. Integer locks snap the selected ",
                    "components. The optional s = w × v marker is calculated from weight × value and ",
                    "cannot be dragged. Overlapping handles keep their positions. Hover a data card to bring its handles in front; the order remains when you leave, and the rings clear."
                ),
            ),
            (
                "Keyboard and history",
                "Tab / Shift+Tab move through input fields and scroll the next field into view. Complex parameters visit Re then Im. Open dialogs keep focus within their own fields. Ctrl+Z undoes; Ctrl+Shift+Z or Ctrl+Y redoes (Command on macOS). Footer Undo/Redo buttons work too. Data, definitions, paths, appearance, view and fitting edits are recorded; an entire drag or colour-picker session is one edit. History stays in memory for this session.",
            ),
            (
                "Expressions",
                "Operators: + − * / % ^ ( ). % is the real remainder; mod(a,b) returns a nonnegative modulo. Constants: pi, tau, e, phi, i. Complex values: 2+3i or complex(2,3). Multiplication is explicit: 2*a. Powers associate right; -2^2 = -4.",
            ),
            ("Math functions", crate::expression::FUNCTION_HELP),
            (
                "Cards and axes",
                "The fixed data header keeps Add row visible and shows total and active counts. Drag ≡ to lift a translucent card preview. Other rows slide aside; release at the highlighted gap to place it. Hold near the list edge to scroll, or press Esc to cancel. Each reorder is one undo step. Hover a card to raise its points; the last hovered set stays in front without pinning. ↑ optionally pins a set in front. Coincident circles keep their exact coordinates. Drag the bottom axis labels left/right to scale x alone; drag the left labels up/down or left/right to scale y. The Input/Output gear controls colours, widths, opacity, dashes and extrapolation.",
            ),
            (
                "Definitions and row references",
                "Use the Definitions table for variables and functions, such as a = 2 and wave(x) = sin(x)*a. Calls work in every numeric field. Function arguments are local and may be real or complex. Forward references work across globals and active data rows; circular dependencies are errors. Use p1[n], p1[v], p1[w], p1[wv] for node, value, weight and weight × value. Select components with p1[n][re] or p1[n][im]. Numeric selectors [0], [1], [2], [3] also work. Existing p1.node, p1.value and p1.weight remain available; data rows require an explicit field. Constants and built-in function names are reserved.",
            ),
            (
                "Multiline numeric functions",
                "Choose Numeric function in Definitions for a multiline body. Use let total = 0; for k in 1..=20 { total += sin(k*x)/k; } then finish with total on its own line, without a semicolon. Local variables are mutable; arguments, globals and loop indices are read-only. Use if re(x) < 0 { -total } else { total } for a conditional result. Comparisons: == != < <= > >=; booleans: && || !, true and false. Branches and boolean operators short-circuit. Ordering and conditions require real values; complex equality works. Loops use integer start..end or start..=end ranges, with at most 10000 iterations and a shared 16384-operation budget per evaluation. Comments use // or #. Outer braces are optional. Enter adds a newline; Tab moves fields; project undo/redo applies to body edits. Both function types can call each other and appear in AAA fit. No while, recursion, return, break or continue; the last expression is the result.",
            ),
            (
                "Unit circle and colour",
                "The Grid / Unit circle gear opens grid and axis styles, spacing, subdivisions, tick labels, Linear / Log10 / Symmetric log transforms, unit-circle strokes and the background colour picker. The Colour gear offers palettes, saturation, magnitude shading, brightness bands, phase/modulus contours, exact resolution and half/double controls. Restore original colouring recovers the original phase wheel and magnitude bands with editable settings. Colour works in real and complex mode. In real mode, node/value/weight imaginary components are zero, the background evaluates r(x+iy), and the overlaid real curve still follows r(x). Stored imaginary expressions are ignored until Complex mode is selected again.",
            ),
            (
                "Input path",
                "Keep multiple named path cards with Add path. Collapse or expand each card with its arrow; cards can stay open together. Real mode offers Real interval and Single point; Complex mode also offers Curve. Complex curves retain their geometry while temporarily displaying as real intervals. Each interval/curve has its own sample count and extrapolation. Add point, × and Clear all edit the selected path. Disabled or invalid paths leave others and colouring available. Real mode evaluates x and retains imaginary coordinates for switching back. Fitting never changes your paths.",
            ),
            (
                "AAA fitting",
                "Define a one-argument function, choose it in AAA fit, and press Run fit. Local continuum sampling inserts probes between support points and reuses cached evaluations. One run stops at tolerance or its support/degree budget, reports verification errors, and waits for you to run again. Settings and definition edits do not start a fit. Choose AAA or NL-AAA (SK/Whitfield nonlinear L2 refinement). Sign modification applies to AAA only. Optional Lawson steps support damping from 0 (uniform weights) to 1 (standard); cleanup is available without Lawson or fixed degree. Adaptive samples and post-processing do not carry a fixed-grid L2 monotonicity guarantee. Fits preview without replacing manual rows and preserve your viewport and every input path, even after switching modes. Frame fit interval explicitly frames the result. Use as editable nodes copies the result. Cancel stops an active fit immediately; Restart fit replaces it. Computations use faer in disposable worker processes.",
            ),
            (
                "Spectrum controls",
                "The grouped Poles / Zeros / Residues toolbar controls visibility. Its gear opens the numerical lists and display filters: all/real/complex locations, pole magnitude limits, and Chebfun cleanup criteria 1/2. Complex roots can appear in real mode at (Re, Im). Frame poles/zeros reveals offscreen locations. Cleanup display filters do not refit the rational function. The toolbar × or Cancel on the spectrum page terminates its worker process. Calculate all or a display toggle resumes analysis. Disabling unneeded outputs cancels their work; residues still need poles and zeros. Panning reuses completed spectra; rendering remains live.",
            ),
            (
                "Nodes and poles",
                concat!(
                    "Active rows define the function. Show only controls marker visibility. ",
                    "Zero weights contribute nothing. Active nodes must be distinct. Exact ",
                    "nonzero-weight nodes return their specified values. Undefined pole ",
                    "samples break the output curve."
                ),
            ),
        ];
        let mut guide = column![
            text("A live rational workbench").size(20),
            text(intro).size(13)
        ]
        .spacing(14);
        for (heading, body) in sections {
            guide = guide
                .push(text(heading).size(16).boxed())
                .push(text(body).size(12).color(MUTED).boxed());
        }
        guide
            .push(text("Try a starting point").size(16).boxed())
            .push(
                row![
                    button(text("Real example").size(12)).on_press(Message::Preset(Mode::Real)),
                    button(text("Complex example").size(12))
                        .on_press(Message::Preset(Mode::Complex)),
                ]
                .spacing(10)
                .boxed(),
            )
            .push(
                text("Examples replace the current workspace. Save a project below to keep it.")
                    .size(11)
                    .color(MUTED)
                    .boxed(),
            )
            .push(self.project_controls())
            .push(space().height(16).boxed())
            .boxed()
    }
    pub(in crate::app) fn project_controls(&self) -> Element<'_, Message> {
        column![
            text("Project file").size(13),
            row![
                text_input("path/to/project.json", &self.path)
                    .on_input(Message::ProjectPath)
                    .size(12),
                button(text("Save").size(12))
                    .on_press(Message::Save)
                    .style(button::secondary),
                button(text("Load").size(12))
                    .on_press(Message::Load)
                    .style(button::secondary)
            ]
            .spacing(7),
            text(&self.notice).size(11).color(MUTED)
        ]
        .spacing(7)
        .boxed()
    }
}
