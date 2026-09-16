//! 表格、分页、时间轴、图表、虚拟列表。

use crate::imports::*;
use crate::shared::*;

pub(crate) fn gal_sample_tree_roots() -> Vec<TreeNode> {
    vec![TreeNode {
        id: "root".into(),
        label: "Workspace".into(),
        children: vec![
            TreeNode {
                id: "src".into(),
                label: "src".into(),
                children: vec![
                    TreeNode {
                        id: "main".into(),
                        label: "main.rs".into(),
                        children: vec![],
                        chrome: Default::default(),
                    },
                    TreeNode {
                        id: "lib".into(),
                        label: "lib.rs".into(),
                        children: vec![],
                        chrome: Default::default(),
                    },
                ],
                chrome: Default::default(),
            },
            TreeNode {
                id: "docs".into(),
                label: "Docs".into(),
                children: vec![TreeNode {
                    id: "readme".into(),
                    label: "README.md".into(),
                    children: vec![],
                    chrome: Default::default(),
                }],
                chrome: Default::default(),
            },
        ],
        chrome: Default::default(),
    }]
}

pub(crate) fn gal_tree_expanded(key: &'static str) -> Signal<HashSet<String>> {
    use_signal(key, || {
        let mut s = HashSet::new();
        s.insert("root".into());
        s.insert("src".into());
        s
    })
}

#[component]
pub(crate) fn TableShowcase() -> Element {
    let composite_sel = use_signal("gal_tbl_comp_sel", || None::<usize>);
    let table_sel = use_signal("gal_dt_sel", || None::<usize>);
    let table_sort = use_signal("gal_tbl_sort", TableSortState::default);
    let table_filter = use_signal("gal_tbl_filter", || String::new());
    let table_page = use_signal("gal_tbl_page", TablePaginationState::default);

    let pick0 = composite_sel;
    let pick1 = composite_sel;
    let pick2 = composite_sel;

    let data_table_el: Element = table("gal-table")
        .columns([
            TableColumn::new("name", "Name").width(120.0),
            TableColumn::new("role", "Role").width(100.0),
            TableColumn::new("score", "Score").width(80.0).align("end"),
        ])
        .row_count(40)
        .height(200.0)
        .caption("TanStack-style · sort · filter · pagination")
        .sort_signal(table_sort)
        .global_filter_signal(table_filter)
        .pagination_signal(table_page)
        .selected_signal(table_sel)
        .cell(|r, c| match c {
            0 => format!("Item {r:02}"),
            1 => if r % 2 == 0 { "Admin" } else { "User" }.into(),
            _ => format!("{}", 60 + (r % 40)),
        })
        .into();

    let filter_input: Element = input("gal-tbl-filter")
        .placeholder("Filter rows…")
        .value_signal(table_filter)
        .w("100%")
        .into();

    ui! {
        <MyCard title="Table · ShadCN composite + TanStack data_table">
            <VStack w="100%" gap="1rem" flex_shrink={0.0}>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "复合 RSX（invoices · caption/header/body/footer · align · hover · selected）+ Builder 预设（VirtualList · 排序 / 筛选 / 分页）。"
                </Label>
                <Label font_weight="600" text_size="0.875rem">{ "Composite · invoices" }</Label>
                <Table>
                    <TableCaption>"A list of your recent invoices."</TableCaption>
                    <TableHeader>
                        <TableRow id="inv-h">
                            <TableHead col_w="100px">"Invoice"</TableHead>
                            <TableHead>"Status"</TableHead>
                            <TableHead>"Method"</TableHead>
                            <TableHead align="end" col_w="96px">"Amount"</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        <TableRow id="inv-0" selected={composite_sel.get() == Some(0)} on_click={move || pick0.set(Some(0))}>
                            <TableCell>"INV001"</TableCell>
                            <TableCell>"Paid"</TableCell>
                            <TableCell>"Credit Card"</TableCell>
                            <TableCell align="end" col_w="96px">"$250.00"</TableCell>
                        </TableRow>
                        <TableRow id="inv-1" selected={composite_sel.get() == Some(1)} on_click={move || pick1.set(Some(1))}>
                            <TableCell>"INV002"</TableCell>
                            <TableCell>"Pending"</TableCell>
                            <TableCell>"PayPal"</TableCell>
                            <TableCell align="end" col_w="96px">"$150.00"</TableCell>
                        </TableRow>
                        <TableRow id="inv-2" selected={composite_sel.get() == Some(2)} on_click={move || pick2.set(Some(2))}>
                            <TableCell>"INV003"</TableCell>
                            <TableCell>"Unpaid"</TableCell>
                            <TableCell>"Bank Transfer"</TableCell>
                            <TableCell align="end" col_w="96px">"$350.00"</TableCell>
                        </TableRow>
                    </TableBody>
                    <TableFooter>
                        <TableRow id="inv-f">
                            <TableCell col_w="100px">"Total"</TableCell>
                            <TableCell></TableCell>
                            <TableCell></TableCell>
                            <TableCell align="end" col_w="96px">"$750.00"</TableCell>
                        </TableRow>
                    </TableFooter>
                </Table>
                <Label text_size="0.875rem">
                    { format!("Composite selected: {:?}", composite_sel.get()) }
                </Label>
                <Label font_weight="600" text_size="0.875rem">{ "data_table · Builder" }</Label>
                { filter_input }
                { data_table_el }
                <Label text_size="0.875rem">
                    { format!(
                        "Selected row: {:?} · sort: {:?} · page: {}",
                        table_sel.get(),
                        table_sort.get().column_id,
                        table_page.get().page_index + 1,
                    ) }
                </Label>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn DataChartShowcase() -> Element {
    let chart_sel = use_signal("gal_chart_sel", || None::<usize>);
    let expanded_col = gal_tree_expanded("gal_tree_exp_col");
    let expanded_flat = gal_tree_expanded("gal_tree_exp_flat");
    let expanded_rsx = gal_tree_expanded("gal_tree_exp_rsx");
    let sample_roots = gal_sample_tree_roots();

    let tree_collapsible_el: Element = tree_view("gal-tree-collapsible")
        .variant(TreeViewVariant::Collapsible)
        .exclusive(true)
        .height(160.0)
        .expanded_signal(expanded_col)
        .expanded_storage_key("gal_tree_exp_col")
        .roots(sample_roots.clone())
        .into();

    let tree_flat_el: Element = tree_view("gal-tree-flat")
        .variant("flat")
        .height(140.0)
        .row_height(32.0)
        .expanded_signal(expanded_flat)
        .expanded_storage_key("gal_tree_exp_flat")
        .roots(sample_roots)
        .into();

    let bar_rows = vec![
        ChartDatum::new("Mon", 0.2),
        ChartDatum::new("Tue", 0.5),
        ChartDatum::new("Wed", 0.8),
        ChartDatum::new("Thu", 0.4),
        ChartDatum::new("Fri", 0.9),
    ];
    let pie_rows = vec![
        ChartDatum::new("Alpha", 32.0),
        ChartDatum::new("Beta", 24.0),
        ChartDatum::new("Gamma", 18.0),
        ChartDatum::new("Delta", 14.0),
        ChartDatum::new("Other", 12.0),
    ];
    let line_vals = vec![0.1, 0.35, 0.55, 0.4, 0.85, 0.7];
    let compose_rows = vec![
        ChartDatum::new("Mon", 42.0),
        ChartDatum::new("Tue", 58.0),
        ChartDatum::new("Wed", 51.0),
        ChartDatum::new("Thu", 73.0),
        ChartDatum::new("Fri", 86.0),
    ];
    let composed: Element = define_chart()
        .id("gal-compose")
        .theme(ChartTheme::tokens())
        .mark(
            area_y(compose_rows.clone())
                .fill("chart-1")
                .fill_opacity(0.28)
                .stroke("chart-1")
                .stroke_width(2.0),
        )
        .mark(
            line_y(compose_rows.clone())
                .stroke("chart-1")
                .stroke_width(2.5),
        )
        .mark(dot(compose_rows.clone()).r(4.0).fill("chart-1"))
        .mark(
            rule_y([60.0])
                .stroke("destructive")
                .stroke_width(1.5)
                .stroke_dash([4.0, 2.0]),
        )
        .y_linear()
        .y_nice()
        .guides(true)
        .tooltip(true)
        .tooltip_indicator("line")
        .crosshair(true)
        .on_select(move |i| chart_sel.set(Some(i)))
        .h(160.0)
        .into();
    let t0 = 1_704_067_200_000i64;
    let time_rows = vec![
        ChartDatum::at(t0, 12.0),
        ChartDatum::at(t0 + 86_400_000, 19.0),
        ChartDatum::at(t0 + 2 * 86_400_000, 15.0),
        ChartDatum::at(t0 + 3 * 86_400_000, 28.0),
        ChartDatum::at(t0 + 4 * 86_400_000, 22.0),
    ];
    let time_chart: Element = define_chart()
        .id("gal-time")
        .mark(
            line_y(time_rows.clone())
                .stroke("chart-2")
                .stroke_width(2.5),
        )
        .mark(dot(time_rows).r(4.0).fill("chart-2"))
        .x_time()
        .y_nice()
        .tooltip(true)
        .crosshair(true)
        .on_select(move |i| chart_sel.set(Some(i)))
        .h(140.0)
        .into();
    let log_rows = vec![
        ChartDatum::new("1", 1.0),
        ChartDatum::new("2", 8.0),
        ChartDatum::new("3", 64.0),
        ChartDatum::new("4", 512.0),
    ];
    let log_chart: Element = define_chart()
        .id("gal-log")
        .mark(line_y(log_rows).stroke("chart-3").stroke_width(2.5))
        .y_log()
        .y_nice()
        .guides(true)
        .on_select(move |i| chart_sel.set(Some(i)))
        .h(120.0)
        .into();
    let radial_rows = vec![
        ChartDatum::new("N", 8.0),
        ChartDatum::new("NE", 5.0),
        ChartDatum::new("E", 7.0),
        ChartDatum::new("SE", 3.0),
        ChartDatum::new("S", 6.0),
        ChartDatum::new("SW", 4.0),
        ChartDatum::new("W", 9.0),
        ChartDatum::new("NW", 2.0),
    ];
    let radial_composed: Element = define_chart()
        .id("gal-radial")
        .mark(radial_bar(radial_rows.clone()).inner_radius(0.2))
        .mark(
            radial_line(radial_rows.clone())
                .stroke("chart-1")
                .stroke_width(2.0),
        )
        .mark(radial_dot(radial_rows.clone()).r(3.5).fill("chart-1"))
        .guides(true)
        .legend(true)
        .tooltip(true)
        .on_select(move |i| chart_sel.set(Some(i)))
        .size(180.0)
        .into();
    let donut_el: Element = donut_chart()
        .id("gal-donut")
        .data(pie_rows.clone())
        .size(160.0)
        .theme(ChartTheme::vibrant())
        .tooltip_indicator("dashed")
        .on_select(move |i| chart_sel.set(Some(i)))
        .into();
    let radial_preset: Element = radial_chart()
        .id("gal-radial-preset")
        .data(radial_rows)
        .size(160.0)
        .theme(ChartTheme::tokens())
        .on_select(move |i| chart_sel.set(Some(i)))
        .into();
    let carousel_el = ui! {
        <VStack w="100%" max_w="36rem" mx="auto" gap="1.25rem">
            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselSingle · autoplay" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "variant=single · autoplay=4000 · loop+seamless · dots · controls=outside"
                </Label>
                <CarouselSingle id="gal-carousel-single" controls="outside" autoplay={4000} loop={true} seamless={true}>
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Slide A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Slide B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Slide C" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselSingle>
            </VStack>

            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselMulti · fraction peek" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "perView=2.2 · itemSizing=fraction · 首项左对齐 · 右侧裁切 · loop+seamless（无 autoplay 便于看初始态）"
                </Label>
                <CarouselMulti id="gal-carousel-multi-slide" perView={2.2} scrollBy="slide" loop={true} seamless={true} itemSizing="fraction">
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "C" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="primary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="primary-foreground">{ "D" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="destructive" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="destructive-foreground">{ "E" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselMulti>
            </VStack>

            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselMulti · fixed + free drag" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "itemSizing=fixed · itemWidth=12rem · snap=false · loop=false · 可随意拖拽"
                </Label>
                <CarouselMulti id="gal-carousel-multi-fixed" itemSizing="fixed" itemWidth="12rem" snap={false} loop={false}>
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "C" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="primary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="primary-foreground">{ "D" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="destructive" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="destructive-foreground">{ "E" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselMulti>
            </VStack>

            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselMulti · seamless + free drag" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "loop+seamless · snap=false · perView=2.2 · 自由拖拽可无限绕圈（无格点吸附）"
                </Label>
                <CarouselMulti id="gal-carousel-multi-seamless-free" perView={2.2} loop={true} seamless={true} snap={false} itemSizing="fraction">
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "C" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="primary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="primary-foreground">{ "D" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="destructive" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="destructive-foreground">{ "E" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselMulti>
            </VStack>

            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselMulti · page + flush" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "scrollBy=page · pageSnap=viewport · endAlign=flush · 整屏推进 · loop=false"
                </Label>
                <CarouselMulti id="gal-carousel-multi-flush" perView={2.2} scrollBy="page" pageSnap="viewport" endAlign="flush" loop={false}>
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "C" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="primary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="primary-foreground">{ "D" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="destructive" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="destructive-foreground">{ "E" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselMulti>
            </VStack>

            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselMulti · page + pad" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "scrollBy=page · pageSnap=peek · endAlign=pad · 裁切尾项成为下页首项 · loop=false"
                </Label>
                <CarouselMulti id="gal-carousel-multi-pad" perView={2.2} scrollBy="page" pageSnap="peek" endAlign="pad" loop={false}>
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "C" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="primary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="primary-foreground">{ "D" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="destructive" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="destructive-foreground">{ "E" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselMulti>
            </VStack>

            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselStack · autoplay" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "autoplay=4000 · stackSides=1 · loop=true · 悬停暂停 · 亦可拖拽吸附"
                </Label>
                <CarouselStack id="gal-carousel-stack-auto" controls="outside" stackSides={1} loop={true} autoplay={4000}>
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Stack A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Stack B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Stack C" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="primary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="primary-foreground">{ "Stack D" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="destructive" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="destructive-foreground">{ "Stack E" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselStack>
            </VStack>

            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselStack · drag" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "无 autoplay · 拖拽跟手 · 松手吸附 · stackSides=1 · loop=true"
                </Label>
                <CarouselStack id="gal-carousel-stack-drag" controls="outside" stackSides={1} loop={true}>
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Drag A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Drag B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Drag C" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="primary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="primary-foreground">{ "Drag D" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="destructive" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="destructive-foreground">{ "Drag E" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselStack>
            </VStack>
        </VStack>
    };

    ui! {
        <MyCard title="TreeView · Chart · Carousel">
            <VStack w="100%" gap="0.75rem" flex_shrink={0.0}>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "树 / Chart（预设 bar/line/area/pie/donut/radial + define_chart；shadcn chrome · tooltip indicator · ChartTheme 色板）/ 轮播。点柱、折线或扇区可选中；tooltip 钉在按下点。"
                </Label>
                <Label font_weight="600" text_size="0.875rem">{ "TreeView · collapsible exclusive（同层互斥 · Builder）" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "递归 Collapsible + chevron-right；默认不互斥。此例 `.exclusive(true)`：同层兄弟目录只开一项。旧 accordion 变体已并入 collapsible。"
                </Label>
                { tree_collapsible_el }
                <Label font_weight="600" text_size="0.875rem">{ "TreeView · flat（VirtualList · Builder）" }</Label>
                { tree_flat_el }
                <Label font_weight="600" text_size="0.875rem">{ "TreeView · RSX collapsible（默认不互斥）" }</Label>
                <TreeView id="gal-tree-rsx" variant="collapsible" expandedSignal={expanded_rsx} expandedStorageKey="gal_tree_exp_rsx" height={140.0}>
                    <TreeViewItem id="root" label="Workspace">
                        <TreeViewItem id="src" label="src">
                            <TreeViewItem id="main" label="main.rs" />
                            <TreeViewItem id="lib" label="lib.rs" />
                        </TreeViewItem>
                        <TreeViewItem id="docs" label="Docs">
                            <TreeViewItem id="readme" label="README.md" />
                        </TreeViewItem>
                    </TreeViewItem>
                </TreeView>
                <Label font_weight="600" text_size="0.875rem">{ "BarChart" }</Label>
                <BarChart
                    id="gal-bar"
                    data={bar_rows.clone()}
                    plot_h="6rem"
                    on_select={move |i| chart_sel.set(Some(i))}
                />
                <Label font_weight="600" text_size="0.875rem">{ "LineChart" }</Label>
                <LineChart
                    id="gal-line"
                    values={line_vals.clone()}
                    plot_h={140.0}
                    on_select={move |i| chart_sel.set(Some(i))}
                />
                <Label font_weight="600" text_size="0.875rem">{ "AreaChart" }</Label>
                <AreaChart
                    id="gal-area"
                    values={line_vals.clone()}
                    plot_h={140.0}
                    on_select={move |i| chart_sel.set(Some(i))}
                />
                <Label font_weight="600" text_size="0.875rem">{ "PieChart" }</Label>
                <PieChart
                    id="gal-pie"
                    data={pie_rows.clone()}
                    on_select={move |i| chart_sel.set(Some(i))}
                />
                <Label font_weight="600" text_size="0.875rem">{ "DonutChart · vibrant · indicator=dashed" }</Label>
                { donut_el }
                <Label font_weight="600" text_size="0.875rem">{ "RadialChart 预设" }</Label>
                { radial_preset }
                <Label font_weight="600" text_size="0.875rem">{ "define_chart · Area+Line+Dot+Rule · tooltip indicator=line" }</Label>
                { composed }
                <Label font_weight="600" text_size="0.875rem">{ "define_chart · 时间轴" }</Label>
                { time_chart }
                <Label font_weight="600" text_size="0.875rem">{ "define_chart · y log" }</Label>
                { log_chart }
                <Label font_weight="600" text_size="0.875rem">{ "define_chart · radial_bar + line + dot" }</Label>
                { radial_composed }
                <Label text_size="0.75rem" color="muted-foreground">
                    { match chart_sel.get() {
                        Some(i) => format!("Chart 选中 index = {i}"),
                        None => "Chart 未选中".into(),
                    } }
                </Label>
                <Label font_weight="600" text_size="0.875rem">{ "Carousel / Multi / Stack" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "复合 Content / Item / Previous / Next / Dots；←→ 或拖拽；弹簧动画。"
                </Label>
                { carousel_el }
            </VStack>
        </MyCard>
    }
}

pub(crate) const GAL_WP310_MASONRY: &str = "gal-wp310-masonry";

#[component]
pub(crate) fn VirtualMasonryShowcase() -> Element {
    let masonry: Element = virtual_masonry(GAL_WP310_MASONRY)
        .item_count(1000)
        .columns(3)
        .gap(8.0)
        .height(280.0)
        .overscan_viewports(1.0)
        .scrollbar_visibility("always")
        .item_height(|i, _| 48.0 + (i % 7) as f32 * 16.0)
        .render_item(|i, w, h| {
            let token = ["primary", "secondary", "accent", "muted"][i % 4];
            Element::from(
                r#box()
                    .w(w)
                    .h(h)
                    .rounded(8.0)
                    .bg(parse_color(token))
                    .p("0.35rem")
                    .text(format!("{i:04}")),
            )
        })
        .into();
    ui! {
        <MyCard title="VirtualMasonry（WP-3.10）">
            <VStack w="100%" gap="0.75rem" flex_shrink={0.0}>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "1000 张变高卡片、3 列最短列优先；只挂视口+overscan。滚动应只换可见格，不掉帧。experimental，不进稳定面。"
                </Label>
                { masonry }
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn TimelineShowcase() -> Element {
    let active = use_signal("gal_timeline", || 2i32);
    let builder_el: Element = timeline()
        .id("gal-timeline-builder")
        .active_item_signal(active)
        .item(
            TimelineEntry::from(("Order placed", "We received your order."))
                .bullet("shopping-cart"),
        )
        .item(TimelineEntry::from(("Packed", "Left the warehouse.")).bullet("package"))
        .item(TimelineEntry::from(("Shipped", "On the way.")).bullet("truck"))
        .item(TimelineEntry::from(("Delivered", "Left at your door.")).bullet("circle-check"))
        .into();

    ui! {
        <MyCard title="Timeline">
            <VStack w="100%" gap="0.75rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "纵向事件轴（对齐 solid-ui timeline.tsx，ShadCN 风格；官方 registry 无此控件）。activeItem=-1 全灰；圆点 i 在 active≥i 时描边 primary，连线在 active≥i+1 时变主色。"
                </Label>

                <Label text_size="0.75rem" font_weight="600">"A. RSX"</Label>
                <Timeline activeItem={active}>
                    <TimelineItem>
                        <TimelineItemBullet>
                            <Icon name="shopping-cart" pointer_events="none" />
                        </TimelineItemBullet>
                        <TimelineItemTitle>"Order placed"</TimelineItemTitle>
                        <TimelineItemDescription>"We received your order."</TimelineItemDescription>
                    </TimelineItem>
                    <TimelineItem>
                        <TimelineItemBullet>
                            <Icon name="package" pointer_events="none" />
                        </TimelineItemBullet>
                        <TimelineItemTitle>"Packed"</TimelineItemTitle>
                        <TimelineItemDescription>"Left the warehouse."</TimelineItemDescription>
                    </TimelineItem>
                    <TimelineItem>
                        <TimelineItemBullet>
                            <Icon name="truck" pointer_events="none" />
                        </TimelineItemBullet>
                        <TimelineItemTitle>"Shipped"</TimelineItemTitle>
                        <TimelineItemDescription>"On the way."</TimelineItemDescription>
                    </TimelineItem>
                    <TimelineItem>
                        <TimelineItemBullet>
                            <Icon name="circle-check" pointer_events="none" />
                        </TimelineItemBullet>
                        <TimelineItemTitle>"Delivered"</TimelineItemTitle>
                        <TimelineItemDescription>"Left at your door."</TimelineItemDescription>
                    </TimelineItem>
                </Timeline>
                <HStack gap="0.5rem" items="center">
                    <Button size="sm" variant="outline" on_click={move || {
                        let s = active.peek();
                        if s > -1 { active.set(s - 1); }
                    }}>"Prev"</Button>
                    <Button size="sm" variant="outline" on_click={move || {
                        let s = active.peek();
                        if s < 3 { active.set(s + 1); }
                    }}>"Next"</Button>
                    <Label text_size="0.875rem">{ format!("activeItem {}", active.get()) }</Label>
                </HStack>

                <Label text_size="0.75rem" font_weight="600">"B. Builder"</Label>
                { builder_el }
            </VStack>
        </MyCard>
    }
}
