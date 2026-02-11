// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use lsp_server::Connection;
    use std::path::PathBuf;
    use sui_move_analyzer::{
        MultiProject, discover_manifest_and_kind, struct_dep_graph::StructDepGraph, utils::*,
    };

    #[test]
    fn test_generic_struct_dep() {
        let (connection, _) = Connection::stdio();

        let mut projects = MultiProject::new();

        let fpath = path_concat(
            std::env::current_dir().unwrap().as_path(),
            PathBuf::from("tests/bugfix/sources/test_generic_dep.move").as_path(),
        );

        // 创建测试文件
        let content = r#"
            module a::m1 {
                struct S1<T> has copy, drop, store {
                    f1: T
                }
            }

            module b::m2 {
                use a::m1::S1;
                struct Account has copy, drop, store {
                    id: u64
                }
                struct S2 has copy, drop, store {
                    // 这里的 S1<Account> 应该被识别到对 Account 的依赖
                    f1: S1<Account>
                }
            }
        "#;

        std::fs::write(&fpath, content).unwrap();

        let (mani, _) = discover_manifest_and_kind(&fpath).expect("not move project.");

        let p = projects
            .load_project(&connection, &mani, Default::default())
            .expect("load project failed");

        projects.insert_project(p);
        let project = projects.get_project(&fpath).unwrap();

        let graph = StructDepGraph::generate_for_project(project);

        eprintln!("Graph JSON: {}", graph.to_json());

        // 验证节点
        let has_node = |name: &str| graph.nodes.iter().any(|n| n.name == name);
        assert!(has_node("S1"), "Missing node S1");
        assert!(has_node("Account"), "Missing node Account");
        assert!(has_node("S2"), "Missing node S2");

        // 验证边 S2 -> Account (通过 S1 的范型参数，标签应为 <>)
        let has_edge = |from: &str, to: &str, label: &str| {
            graph.edges.iter().any(|e| {
                graph.nodes.iter().any(|n_from| {
                    n_from.name == from
                        && format!("{}.{}.{}", n_from.address, n_from.module, n_from.name) == e.from
                }) && graph.nodes.iter().any(|n_to| {
                    n_to.name == to
                        && format!("{}.{}.{}", n_to.address, n_to.module, n_to.name) == e.to
                }) && e.field_names.contains(&label.to_string())
            })
        };

        assert!(
            has_edge("S2", "S1", "f1"),
            "Missing edge S2 -> S1 with label f1"
        );
        assert!(
            has_edge("S2", "Account", "<>"),
            "Missing edge S2 -> Account via generic param with label <>"
        );

        // 清理
        std::fs::remove_file(&fpath).unwrap();
    }
}
