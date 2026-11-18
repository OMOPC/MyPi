# 通用模块视图：设计简报

本文是 `read` 模块视图的语言契约设计。读完应当能直接动手，不必回头读现有实现。未决项集中在文末。

## 一、背景

两件工具都消费语言知识，方式不同：

- `grep` 消费"关系"：一次 `A::b` 查询，语言在调用方给好的文件集合里算出若干行文本（xray）。它不读盘。
- `read` 消费"行表"，而且需要这个文件之外的信息：一个文件的邻域（它所属单元的入口、引用它的文件）不在这个文件里，必须去别处取。

模块视图目前是 Rust 专有的：入口文件（`mod.rs`、双轨 `x.rs`、`lib.rs`）与"目录即模块"的判断写死在视图的实现里，四段 `exports / down / up / scan` 也是 Rust 的叙述方式；非 `.rs` 的文件根本不产生视图。

反例说明四段不是通用形状：Markdown 的 `[[wikilink]]` 体系里，一条笔记有出链、反链、同目录兄弟笔记，但没有"导出"，也没有"入口文件"。它的邻域是一张双向图。

结论：模块视图不是四个槽位，而是"某个文件的邻域"；槽位是语言自己选的叙述方式。

## 二、目标

1. 新增语言 = 一个文件（事实）+ 一行登记，视图随之出现；语言不提供就没有视图。
2. 语言只交事实，机器只做图算法，工具只贴行。
3. Rust 那套段落是这套契约的一个实例，不是契约本身。
4. 会话记忆与"什么时候问"归工具（`read`），机器只给结果。

## 三、非目标

- 不做编译器级语义：宏生成、`include!`、`#[path]`、`cfg` 的求解不在这一层。
- 不追求跨语言同构的行集：某门语言没有"导出"，视图里就不该出现导出。
- 不在机器层做会话记忆与触发策略。

## 四、三层分工

| 层 | 位置 | 碰不碰盘 | 认不认语言 |
|---|---|---|---|
| 事实 | `lang/guages/<lang>/` | **只读**，经 `Loader` | 就是语言本身 |
| 机器 | `lang/graph/` | 读，同一个 `Loader` | 不认，只调事实表的函数指针 |
| 消费者 | `read`、`grep` | 不碰 | 不认 |

事实函数是只读的，但**可以拿加载器**。一门语言算出一条引用落向哪个文件，往往必须查盘：Markdown 的 `[[笔记A]]` 要按名字在树里找，Go 的 import 要读 `go.mod` 才知道当前模块名，Python 的 `from pkg import x` 要定位包根。因此契约给事实函数一个只读 `Loader`，而不是把它约束成"纯文本变换"。

```text
Loader {
    entries(dir: &Path) -> Vec<Entry>,      // 一层目录的条目，已排序、已跳过垃圾
    read(path: &Path)  -> Option<String>,   // 一个文件的内容；不存在或不可读给 None
}
Entry { name: String, path: PathBuf, dir: bool }
```

只有这两个动作，不写、不建、不删。机器与语言共用同一个，于是"能读什么、跳过什么、多大算太大"只有一处实现。`Loader` 内部可以缓存本次已读的文件（同一文件不读两次），但缓存只活在一次调用内，不是跨调用的会话记忆。

目录列表还要一份不过滤的条目（点文件、`target` 也要显示）：`graph` 另出 `entries_all`，与 `Loader` 的过滤同一实现，只差一个开关。

`entries` 的跳过与排序也归 `Loader`：跳过 `target`、`node_modules` 与点开头的条目，目录在前、名字不区分大小写。`read` 对超过 4 MiB 的文件给 `None`，与 `grep` 的现状一致。语言与机器都不各自实现这份名单。

依赖方向：`read → lang`，`read → lang::graph`，`guages/<lang> → lang::graph`。机器不引用任何具体语言（事实表以参数传入），不成环。`read` 从 `lang::owner` 取到 `Language.hub`，把事实表交给 `lang::graph`，自己不认识任何语言。

`read` 的目录列表还要问"这个目录是不是源码树"，经 `lang::tree(dir)`：各语言的 `domain` 对目录作答，`read` 因此不必知道 `Cargo.toml`、`src/` 这些名字。

## 五、语言交出的五种知识（六个函数位）

每件事一个函数，拓扑一项拆成两个。只有 `edges` 是必需：没有边就没有图。其余缺了就降级（见第八节）。

```rust
struct Hub {
    edges:     fn(&Path, &str, &Loader) -> Vec<RawEdge>,      // 必需
    unit_of:   Option<fn(&Path) -> Option<Unit>>,
    territory: Option<fn(&Unit, &Loader) -> Vec<Decl>>,
    interface: Option<fn(&Path, &str) -> Vec<String>>,
    domain:    Option<fn(&Path) -> Option<Domain>>,
    view:      Option<fn(&Around, &Loader, &BTreeSet<PathBuf>) -> Vec<Row>>,
}
```

`territory` 要读文本，但**不收 `Table` 参数**：语言在自己的文件里用自家分拣员建表即可，契约只给 `Loader`。

这份 `Hub` 挂在 `Language.hub: Option<&'static Hub>` 上；`None` 表示这门语言没有模块视图，`read` 据此直接跳过。

`view` 的第三个参数是本次调用一起读的文件集合，供 `[body in this batch]` 这类跨条目注记使用；机器只透传，不解释。

### 5.1 拓扑：什么是"一个单元"

```rust
Unit { name: String, place: PathBuf, entry: Option<PathBuf> }
```

- 单元身份 = `place`（规范化路径）。`entry` 只用来读文本，可为空。
- `place` 是文件或目录，**单元不等于入口文件**：Go 的单元是目录（`entry` 为空），Markdown 的单元是笔记本身（`place` 是那个文件），Rust 的单元是目录加 `mod.rs` / 双轨 `x.rs` / `lib.rs`。
- `unit_of` 决定一个文件属于哪个单元。默认：文件自己是单元（`name` 取文件 stem）。
- `territory` 决定一个单元辖下有哪些子单元，**允许看文本**（Rust 的辖区来自入口文件里的 `pub mod x;`，需要读 `entry` 的文本）。默认：`place` 下的一层子目录，各自成为一个子单元；`place` 是文件时默认无辖区。
- `territory` 返回的不只是单元，还有"父级是怎么写的这一条"：

```rust
Decl { unit: Unit, line: Option<usize>, tags: Vec<String> }
```

- `line`：父级在哪一行声明了它（Rust 就是 `pub mod x;` 那一行）；拿不到行就留空。
- `tags`：语言自己的词，机器只印不解释（Rust 放 `["pub"]` 或 `["private"]`，Go 与 Markdown 放空）。可见性这类只有个别语言才有的概念因此不必进契约。`hub`/`leaf` 不进标签：它由 `has_children` 判定（第六节）。
- 机器用 `territory(中心)` 得 `children`；再用 `territory(父单元)` 找出指向自己的那条 `Decl`，得 `declared_by`（第七节）。

### 5.2 出边与落点：它指向谁

单看一份文本，语言能抽出若干外部引用意图；但意图还不是边，必须落到目标路径。两步共享同一套语法上下文（Rust 的 `crate`/`self`/`super` 要先知道写这一行的文件在哪个单元），所以合成一个函数：

```rust
RawEdge {
    line: usize,             // 写这一行的行号，0 基
    wrote: String,           // 引用原文，用于展示外部目标
    symbols: Vec<String>,    // 这条边带过来的名字；空 = 只知道有关系
    targets: Vec<PathBuf>,   // 有序候选，已穿过转发；空 = 落不到本树（外部）
    via: Vec<PathBuf>,       // 穿透时经过的转发单元入口，按经过顺序
    re_export: bool,         // 这条边本身就是一次转发声明（Rust 的 pub use）
}
```

- `targets` 有序（`a/b.rs` 在前、`a/b/mod.rs` 在后），机器取第一个存在的。
- `targets` 为空表示"指向外部"（`use serde::X`、Go 的外部模块）。这类边不丢：视图要能说"外部"，而不是谎报"没人引用"。
- `via` 由语言填。把"写的路径"翻译成"真实落点"要懂自家模块名到文件的映射，还要读别的文件，这是语言知识；机器只把它当显示材料，不重做第二遍。
- `re_export` 仍要留：机器据此不把转发声明算成"引用方"，否则 `pub use` 会被当成消费者。

### 5.3 接口：节点对外长什么样

```rust
interface: Option<fn(file: &Path, text: &str) -> Vec<String>>   // 一行一个签名，已是可打印文本
```

Rust / Python / Go 给公开符号；Markdown 给标题树（标题就是笔记对外暴露的接口）。可空。

### 5.4 范围：去哪里找"谁指向我"

出边顺着当前文件就能找到；入边必须扫别的文件。语言只给"搜索域是什么"，怎么走归机器。

```rust
Domain { root: PathBuf, shape: Shape }
Shape::Levels   // 从单元目录起逐层向上，到 root 为止（Rust: Cargo.toml 所在目录）
Shape::Tree     // root 之下整棵树（Markdown 的库根、Go 的 go.mod 目录、Python 的包根）
```

根标志由语言给：工具不知道 `Cargo.toml` 与 `go.mod`、`pyproject.toml` 的区别。`domain` 也能对目录作答，`read` 据此判断一个目录是否在源码树里。返回 `None` 表示这门语言没有入边可找。

### 5.5 叙述：怎么把材料排成文字（可选）

```rust
view: Option<fn(around: &Around, loader: &Loader, batch: &BTreeSet<PathBuf>) -> Vec<Row>>
```

材料的形状固定（第六节），怎么分段、用什么措辞、要不要"导出"这一段，由语言决定。不填就用机器的默认叙述（第九节）。

## 六、机器算出来的材料：Around

```rust
Around {
    unit: Unit,
    file: PathBuf,              // 模型刚读的那个文件；tag 与 batch 都由它判断
    base: PathBuf,              // 路径显示的基准（Rust: crate 根）；语言不给就用 unit.place
    interface: Vec<String>,     // 这个单元对外给出什么
    children: Vec<Child>,       // 辖下子单元
    declared_by: Option<Decl>,  // 上级怎么声明了我（Rust 的 pub mod x; 那一行）
    outgoing: Vec<Edge>,        // 它指向谁；to 为空即外部
    incoming: Vec<Edge>,        // 谁指向它；to 一律归一到本单元
    boundary: Boundary,         // 走到哪停的
}

Decl   { unit: Unit, line: Option<usize>, from: Option<PathBuf>, tags: Vec<String> }
Child  { unit: Unit, size: Size, has_children: bool, tags: Vec<String>,
         interface: Vec<String>, outgoing: Vec<Edge> }
Size   { Lines(usize), Files(usize), Unknown }     // 机器用 Loader 数出来，不由语言给
Edge   { at: PathBuf, to: Option<Unit>, to_file: Option<PathBuf>, wrote: String,
         line: usize, symbols: Vec<String>, re_export: bool, via: Vec<PathBuf> }
Boundary { levels: usize, files: usize, stop: Option<PathBuf>, complete: bool, no_above: bool }
Row    { heading: Option<String>, text: String }    // 沿用 lang::Row
```

- `base` 是所有相对路径的基准。它来自 `domain` 的根；没有 `domain` 时退到 `unit.place`。
- `Edge.at` 是这一行写在哪个文件；`Edge.to_file` 是它落在哪个文件。`up` 要印"文件:行"，子单元的横向依赖要按文件分兄弟，两个粒度都不能丢。
- `Decl.from` 是这条声明写在哪个文件（父单元的入口），`declared_by` 再按 `entry` 或 `place` 对回自己。
- `incoming` 的 `to` 一律归一到本单元：消费者写 `a::b::c` 也算指向 `a::b`。合并键 `(at, to)` 归一后等价于"按文件合并"：同一文件里多条指向本单元的引用只成一行，符号按出现顺序合并，行号取首次。
- `children` 的每一项都带三样东西：体量（`size`）、有没有下级辖区（`has_children`，机器问一次 `territory` 即得）、以及 `outgoing`——它指向本单元辖区内的文件。`outgoing` 只留落在辖区内的目标，否则一个子节点就能把整张图拖进来。
- `tags` 是语言的词，机器只印不解释：Rust 只放 `["pub"]` 或 `["private"]`，Go 与 Markdown 放空。`has_children` 是通用的"有没有下级辖区"，`hub`/`leaf` 由此判定，不由语言给词。
- `declared_by` 就是 `up` 段顶部那一行（Rust 的 `src/a/mod.rs:12 declares b`）：机器拿 `territory(父单元)` 里指向自己的那条 `Decl`。
- 不单列 `external` 列表：`outgoing` 里 `to: None` 的就是外部，视图自己决定显示与否。
- `via` 是穿透路上经过的转发单元入口，语言解析落点时已经填好（5.2），机器只搬运与截断，最多显示到 8 跳。
- `boundary.stop` 是停下的目录（`complete` 为假时才有意义），`complete` 表示走到了域根，`no_above` 表示单元本身就是域根。这三样让叙述能说"停在哪、为什么停"，而不只是"停了多少层"。
- `size` 由机器数（目录给文件数、文件给行数），语言不参与。

## 七、机器的三步投影

先定一条贯穿全程的规则——**单元文本**：`entry` 指向的文件；没有入口且 `place` 是文件，就是它；`place` 是目录（如 Go 的包）就是目录下所有文件。取接口、找出边都用它。

1. **定心**：`unit_of(文件)` → `unit`；拿不到就退化为"文件自己是单元"。
2. **展开辖区**：`territory(unit)` 得到 `Decl` 列表；每个 Decl 的单元按单元文本取 `interface`、用 `Loader` 数 `size`、再问一次 `territory` 得 `has_children`，组成 `children`。
3. **连接路口**：
   - **父级声明**：`unit_of(unit.place 的父目录)` 得父单元，在 `territory(父)` 里找 `place` 指向自己的那条 `Decl`，记成 `declared_by`；找不到就留空。
   - 中心单元的每份单元文本跑 `edges` → `outgoing`：语言已经穿过转发，`targets` 就是真实落点。机器取第一个存在者、`unit_of` 化归成目标单元；落不下即 `to: None`（外部）；同一目标的边合并符号。
   - 每个子单元的单元文本跑 `edges` → `Child.outgoing`，只留 `to` 落在本单元辖区内的那些，同一目标合并。
   - 按 `domain` 划定搜索域：`Levels` 逐层向上（第一层必完成，其后受截止），`Tree` 扫整棵树；两种都**跳过本单元自己的目录**。域内每个文件跑 `edges`，`to` 等于本单元或落在本单元之下的收为 `incoming`，`to` 归一到本单元，按 `(at, to)` 合并成一行。`Boundary` 顺带记下停在哪、走了几层、看了几个文件。
   - 穿透不再由机器做：语言在 `edges` 里已经走完转发链并把 `via` 填好，机器只负责搬运与截断。

三步之外的事（排序、去重、计数、截断、分组）属于叙述，不属于构图。

## 八、降级：缺了会怎样

| 缺失 | 视图变成 |
|---|---|
| 语言不提供 `Hub` | 没有模块视图（`Language.hub` 为 `None`），其余工具照常 |
| `domain` 不填或返回 `None` | 没有 `incoming`；`base` 退到 `unit.place` |
| `territory` 不填 | `children` 由"一层子目录"推出；`declared_by` 为空，`tags` 与 `has_children` 也没有，`hub`/`leaf` 一律按 leaf 印 |
| `interface` 不填 | 不出现"接口"段 |
| `unit_of` 不填 | 一个文件就是一个单元 |
| `view` 不填 | 机器的默认叙述 |

## 九、默认叙述：机器给零件，语言自拼

机器只提供**排版零件**，不提供叙述范式；再以这些零件提供一个 `default_view` 作为现成组合。

```rust
parts::heading(label, count)   -> String   // "up (12):" / "interface (3):"
parts::item_line(text)         -> String   // "  pub fn rows(&self) -> Vec<Row>"，接口一行
parts::node_line(name, size)   -> String   // "  a/  3 files" / "  a.rs  40L"
parts::edge_line(base, edge)   -> String   // "  src/a/b.rs:12 imports B" / "  … serde (external)"
parts::joined_all(items)       -> String   // 一条不漏，用 "; " 连接
parts::joined(items)           -> String   // 300 字符封顶，余量记作 [+N more]
parts::grouped(base, edges)    -> Vec<String> // "src/a (12 files): b.rs:3, c.rs:9, …"
parts::boundary_line(base, b)  -> String   // "[scan: up→… | 2 levels, 41 files looked at]"
parts::clip(text)              -> String   // 单行超 400 字符截断，尾部加 [+N]
```

`default_view` 就是这些零件的预置排列：接口一段、`children` 一段、`outgoing` 一段、`incoming` 一段、边界一段；`incoming` 超过 200 条时走 `grouped`。`children` 的一行由 `node_line` + `tags` + `has_children`（印 `hub` 或 `leaf`）+ `interface` 拼出，子单元落在辖区内的 `outgoing` 跟在同一行尾；`declared_by` 若有，单独印在 `incoming` 段之前。Rust 不满足于此，就自己写 `view`；别的语言不必跟着 Rust 走。

## 十、各语言填写示意

| 语言 | 拓扑（单元/辖区） | 出边与落点 | 接口 | 范围 |
|---|---|---|---|---|
| Rust | 目录+入口；辖区来自入口里的 `pub mod x;` | `use` 树 → `a/b.rs`、`a/b/mod.rs`；`pub use` 标 `re_export` | `pub` 项与 `pub use`，两样都给；中心与子项同一规则 | Levels，`Cargo.toml` 所在目录；也答目录，供列表闸门 |
| Markdown | 笔记即单元，无辖区 | `[[x]]` → 树内同名 `.md` | 标题树 | Tree，库根 |
| Python | 包目录+`__init__.py`；辖区为子包 | `import`/`from` → `.py`、`__init__.py` | `__all__` / 下划线规则 | Tree，`pyproject.toml` 或包根 |
| Go | 目录即单元，无入口、无辖区 | import 模块路径 → 目录 | 大写标识符 | Tree，`go.mod` 目录 |

"——"即留空，机器降级。这张表就是"支持什么就抽象成什么"的落地形式。

## 十一、已知缺口

1. 通配转发（`pub use a::*`）在数据上表达不出符号，穿透时无法对照，只能不追；这类边仍会作为普通边存在。
2. 外部目标只作为 `to: None` 的边存在，视图能说"外部"，但不会去解析外部仓库。
3. 辖区接口要求为每个子单元取接口，Rust 现状就是"每个子文件读一遍"。Go 这类一包几百文件的语言接入时，`children` 的接口需要上限。
4. `Child.outgoing` 要求为每个子单元跑一遍 `edges` 并解析落点；Rust 现状的 `deps` 已是这个成本。子单元很多时要设上限，否则一个宽单元会把每次读变成一次全量分析。

## 十二、预算、记忆与触发

- 图的**行数预算**由消费者给：视图单独一份（800 行 / 256 KiB），与正文分开计，不合并。
- 机器只回答"这次读了哪些文件、是否截断、截了多少"（`Boundary`），自己不裁总行数；标题与 `out`/`up` 行超 400 字符截断，`exports` 与子项行不截断，分组属于默认叙述的零件。
- 向上扫描的默认：第一层必完成，其后受 300ms 截止；超时的层整层丢弃（半层会被读成"没人引用"）。数值在 `walk.rs` 里，消费者不传。
- 会话记忆留在 `read`，机器不持有状态。触发规则与语言无关：
  1. 这个文件是所属单元的入口（`unit.entry == Some(file)`）→ 一律给视图，标记 `entry file`。
  2. 否则本单元在本会话还没给过 → 给一次，标记 `primer`。
  3. 非入口文件里出现 `mod x;` 时，该文件成为它所在单元的入口：没有入口文件的目录也有视图，且用它自己的文本。
- 只有模型实际 `read` 的那个文件盖 `License` 戳。视图顺带读的文件不盖——盖上等于绕过写许可，`splice` 会用视图当授权。

## 十三、Rust 事实要点

- 单元：`src/` 之下的路径。文件在 `src/a/b.rs` 且 `src/a/b/` 是目录时，单元是那个目录（双轨）：`place` 取目录，`entry` 取 `b.rs`；目录单元另有 `mod.rs`，根部是 `lib.rs` / `main.rs`。
- 辖区：入口文本里的 `mod x;`，先找 `x/mod.rs` 再找 `x.rs`；双轨文件的子模块在它旁边的 `x/` 目录里解析。没有声明时按目录内的 `.rs` 文件回退，排除入口自身。
- 导出：`pub` 项（只取签名，排除 `use` / `impl` / `mod`）与 `pub use` 行，两样都给。
- 边：`use` 树展开成完整路径，认 `crate` / `self` / `super` 与同目录相对名；落点取"最长前缀命中文件"，先 `a/b.rs` 再 `a/b/mod.rs`；`pub use` 标 `re_export` 并参与转发穿透，`via` 记经过的入口文件。
- 范围：向上的域根是 `Cargo.toml` 所在目录，逐层走；扫描跳过本单元目录与非本语言后缀。

`Language` 现有字段不动，只新增 `hub`；`Table` 与两个分拣员的规则不改。

## 十四、落地形状

新建（`lang/graph/`，机制层，不认语言）：

| 文件 | 职责 |
|---|---|
| `mod.rs` | 对外只出 `unit(facts, file) -> Option<Unit>`、`build(facts, loader, file) -> Option<Around>` 与 `render(facts, around, loader, batch) -> Vec<Row>` |
| `loader.rs` | `Loader`（fs 实现 + 内存实现，后者供探针与测试用）与 `entries_all` |
| `index.rs` | 边归并：取首个存在落点、`unit_of` 化归、按 `(at, to_file)` 合并、incoming 归一 |
| `walk.rs` | `Levels` / `Tree` 两型搜索域、截止、整层丢弃、`Boundary` |
| `parts.rs` | 第九节的排版零件，含 `grouped` |
| `view.rs` | `default_view`（零件的预置组合） |

改造（`lang/guages/rust/`）：

| 文件 | 内容 |
|---|---|
| 新增 `facts.rs` | 拓扑（入口、模块链、辖区、无声明回退）、`interface`、`edges`（含转发穿透与 `via`）、`domain` |
| 新增 `view.rs` | 五段叙述；旧文本 + 本节列出的经批准改动 |
| 现有 `relations.rs` | 不动（xray）；与 `facts.rs` 的合并见第十五节 |
| 现有 `table.rs` / `scan.rs` / `format.rs` / `check.rs` | 不动 |

**Rust `view` 的文本规格**

段序：`exports`、`down`、`out`、`up`、`scan`。

- 标题：`--- module a::b (src/a/b/) · primer|entry file ---`；模块链为空时印 `crate`。
- `exports (N):`：入口文本的 `pub` 项与 `pub use` 行，两样都给；都没有才是 `none`。中心与子项共用这条规则。
- `down (N):` 每子单元一行：`  {label} {size} {tags} {hub|leaf} exports (M): {items}`，其后可选 ` [body in this batch]`、` deps: …`。`label` 取子单元相对本单元的路径首段：目录带 `/`，文件带 `.rs`（双轨文件仍是 `x.rs`）。`size` 目录数文件、文件数行。`hub`/`leaf` 由 `has_children` 决定。入口没有 `mod` 声明时退化为本目录的 `.rs` 文件列表（排除入口自身），可见性印 `?`。`deps` 以子单元自己的目录为基准，列它够到的兄弟及其符号。
- `out (N):` 中心自己的出边，位于 `down` 之后：`  {相对路径}:{行号} imports {符号}`；外部目标印引用原文并加 ` (external)`；`pub use` 不进此段；0 条印 `out (0): none`。
- `up (N):` 第一行是父模块对本模块的声明（`declared_by`），其后每个引用方一行：`{相对路径}:{行号} imports {符号}`，有转发时 ` via {相对路径}`。超过 200 条走 `parts::grouped`，只印 `name:line`。
- 末行：`[scan: up→{…} | down→{…} | {N} levels, {M} files]`；前两段由 `boundary.stop` / `complete` / `no_above` 决定措辞。
- 整段 800 行 / 256 KiB 截断，尾部 `[+N lines]`。
- `down` 的每个子单元都要读文本取接口与出边；这是构造所需的成本，不是新增负担。

收缩（`read/`）：

- `read/` 不再包含语言知识：Rust 的事实与叙述在 `guages/rust/`，机制在 `lang/graph/`。
- `read/tool.rs` 只留：何时问（触发与 `seen` 记忆）、预算、盖戳。

**验收**：不再要求与旧版逐字节相同。对同一批文件比较新旧输出，差异只允许落在下面七条；其余行必须逐字节相同。

1. 中心 `exports`：`pub use` 与 `pub` 项两样都给，不再只认 `pub use`。
2. `hub`/`leaf` 改按 `has_children` 判，没有子模块的目录模块不再冒充 hub。
3. 无声明回退不再把入口文件自身列为子项。
4. `scan` 行新增层级与文件数。
5. 新增 `out` 段。
6. 老式布局（无入口文件、其中某文件声明 `mod`）改用该文件自身文本，视图从空变真。
7. `down` 的 `deps` 增加“落在辖区的更深目标”，符号按落点合并；双轨文件的子模块在它旁边的目录里解析。

探针按"归一化后比对"实现：去掉 `out` 段、替换 `scan` 行规模、把第 1 条的 `none` 归一，再 diff；第 2、3、6 条在语料里单独点名核对。探针用完即删。

## 十五、未决（都不阻塞）

1. Markdown 的域根尚未定（`.obsidian`、`.git`、工作区根三选一）；等实现 Markdown 时定。Markdown 暂不提供 `Hub`。
2. xray（`grep` 的 `relations`）与 Rust 的事实各自解析可见性与名字，后续可合并成一份，减少两处规则漂移；不在本次范围。

命名已定：`Hub`、`Around`、`Decl`、`Child`、`Edge`、`Boundary`，机制层在 `lang/graph/`，Rust 的事实与叙述在 `guages/rust/facts.rs` 与 `guages/rust/view.rs`。
