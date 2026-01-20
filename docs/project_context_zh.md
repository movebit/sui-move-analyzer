这段代码是 Move Analyzer（MoveBit）的核心部分，定义了 `ProjectContext`——整个类型解析、符号查找、作用域管理的“大脑”。


------------------------------------------------
一、ProjectContext 是什么？
------------------------------------------------
1. 作用  
   保存“当前分析到哪儿了”的所有状态：  
   - 嵌套的作用域（`scopes: Vec<Scope>`）  
   - 所有地址-模块的符号表（`addresses: Addresses`）  
   - 当前正在分析的模块地址和名字（`addr_and_name`）  
   - 当前处于哪种语法环境（`access_env: Move | Test | Spec`）  

2. 生命周期  
   每打开一个源文件、每进入一个函数、每进入一条 `spec` 块，都会新建或复用 `ProjectContext`，保证“看不到不该看”的符号。

------------------------------------------------
二、三大核心能力
------------------------------------------------
1. 符号进入（enter_*）  
   把新解析到的 `Item`（函数、结构体、常量、use 语句等）写进当前作用域或顶层模块表。  
   - `enter_item` → 普通局部变量 / 函数参数  
   - `enter_top_item` → 模块级函数 / 结构体  
   - `enter_use_item` → use 语句带来的别名  

2. 符号查找（find_*）  
   按“先局部再全局、先 use 再定义”的顺序把符号找出来。  
   - `find_name_chain_item` → 处理 `foo::bar::baz` 这种链式名字  
   - `find_name_corresponding_item` → 处理“方法调用”语法 `s.foo()`，先拿到 `s` 的类型，再在它对应的模块里找名为 `foo` 且第一个参数类型匹配的函数  
   - `find_name_chain_ty` → 仅把符号当成类型用（结构体、类型参数、built-in）

3. 类型解析（resolve_type）  
   把 AST 里的 `Type`（源码写法）变成 `ResolvedType`（内部带解析结果）。  
   - 遇到 `vector<T>` 直接展开成 `ResolvedType::Vec(elem_ty)`  
   - 遇到 `Foo<Bar>` 先通过 `find_name_chain_ty` 找到 `Foo`，再把 `Bar` 填到它的类型参数里  

------------------------------------------------
三、作用域管理小技巧
------------------------------------------------
1. 进入/离开自动成对  
   `enter_scope` 返回一个 `ScopesGuarder`，RAII：  
   ```rust
   let _guard = ctx.enter_scope_guard(...);
   // 离开作用域时自动 pop，不会漏
   ```

2. 三种环境隔离  
   - `AccessEnv::Move` 正常代码  
   - `AccessEnv::Test` 可见 `#[test]` 标记的函数/结构体  
   - `AccessEnv::Spec` 可见 `spec` 专用函数、schema、原生规范函数  
   所有 `collect_*` 系列函数都会根据 `self.get_access_env()` 过滤掉不该出现的符号。

------------------------------------------------
四、666 行附近那段 `if item_ret.is_none() { ... }` 在干嘛？
------------------------------------------------
场景：  
```move
use std::vector;   // 只写了 use，没有 use 具体成员
...
let v = vector::empty();   // 链式名字，但 vector 不是模块而是 use 别名
```
当 `find_name_chain_item` 在局部作用域和 use 表里都没找到对应成员时，会退而求其次：  
1. 把路径第一段当成“模块名”去全局地址表里搜（`project.name_to_addr_impl`）。  
2. 如果搜到地址，再依次把后续段当成模块名/成员名继续往下找。  
3. 找到后把 `module_scope` 设成该模块的 `AddrAndModuleName`，`item_ret` 设成对应 `Item`。  

一句话：**补全“只有 use 模块、没有 use 成员”时的链式查找。**

------------------------------------------------
五、常见二次开发入口
------------------------------------------------
| 需求 | 入口函数 |
|---|---|
| 新增一条内置类型 | `resolve_type` 里加分支 |
| 让分析器识别新语法块的符号 | 新增 `enter_xxx_item`，并在解析 AST 时调用 |
| 调整“方法调用”查找规则 | 改 `find_name_corresponding_item` |
| 新增一条可见性规则 | 改 `item_access_able` |

------------------------------------------------
六、调试锦囊
------------------------------------------------
1. 打开 trace  
   设置 `RUST_LOG=move_bit::project_context=trace`，能在控制台看到每次 `enter_item` 和 `find_name` 的详细日志。  
2. 打印作用域  
   在 `inner_first_visit` 闭包里 `println!("scope={:#?}", s)`，可观察符号表逐级查找过程。  
3. 单测快速验证  
   在 `move_bit/tests/` 里新建一个小 `.move` 文件，写一条你想测的 use/调用/类型，跑 `cargo test` 即可断点。

------------------------------------------------
七、小结
------------------------------------------------
`ProjectContext` 就是 Move 语义分析的“中央数据库 + 搜索引擎”。  
- 写符号 → 各种 `enter_*`  
- 读符号 → 各种 `find_*`  
- 类型计算 → `resolve_type`  
- 作用域生命周期 → `ScopesGuarder`  

把这张“地图”记住，二次开发或定位 bug 就能做到“指哪打哪”。