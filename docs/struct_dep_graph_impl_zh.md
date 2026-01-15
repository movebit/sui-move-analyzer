# 结构体依赖图设计记录

## 目标
为 Move 项目生成“结构体-字段-被引用结构体”的有向图，用于后续静态检查与可视化。后续还会增加"实例化的范型结构体用到的类型结构体"的引用关系图。

## 总体思路
1. 复用现有 `Project` 遍历框架，不另起炉灶。  
2. 依赖库只负责把类型信息注册到 `ResolvedTable`，不产生节点；节点与边仅对当前 workspace 里的源码包采集。  
3. 同一条边如果由多个字段触发，先把字段名收进 `HashSet`，最后排序拼成逗号分隔字符串，保证图边唯一且可读。

## 核心组件

### StructDependencyVisitor
实现 `ItemOrAccessHandler`，只做两件事：
- 碰到 `Item::Struct` 时记录节点；  
- 扫描每个字段，对 `ResolvedType` 递归拆包，把找到的 struct 引用登记成边。

关键伪代码：
```rust
fn on_struct(&mut self, s: &Struct) {
    let from = format!("{}::{}", s.module, s.name);
    self.nodes.insert(StructNode { id: from.clone() });

    for field in &s.fields {
        self.extract_dependencies(&from, &field.name, &field.ty);
    }
}

fn extract_dependencies(&mut self, from: &str, field: &str, ty: &ResolvedType) {
    match ty {
        Struct(id, type_args) => {
            let to = format!("{}::{}", id.module, id.name);
            self.edges.entry((from.into(), to))
                      .or_default()
                      .insert(field.into());
            for arg in type_args {  // 处理嵌套
                self.extract_dependencies(from, field, arg);
            }
        }
        Vector(inner) => self.extract_dependencies(from, field, inner),
        Reference(inner) => self.extract_dependencies(from, field, inner),
        _ => {}
    }
}
```

### generate_for_project 编排
1. 先按依赖顺序遍历第三方包，用 `DummyHandler` 填充 `ResolvedTable`；  
2. 再对根 `Manifest` 启动 `StructDependencyVisitor`，此时所有类型已能解析；  
3. 收集完成后，把 `edges: HashMap<(String,String), HashSet<String>>` 展开成 `Vec<StructEdge>`，字段名排序后合并。

## 边合并示例
假设结构体 `User` 有两个字段：
```move
name: String,
description: String,
```
最终只产生一条边：
```
User -> String, 字段名: "description, name"
```
