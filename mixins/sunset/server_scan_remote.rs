// Read the server's actual command dispatcher, never the vanilla fallback.
use crate::game::Game;
use horizon_client::command::{Dispatcher, NodeKind, Suggests};
use std::collections::VecDeque;
use std::sync::Mutex;

pub(crate) struct View {
    pub address: String,
    pub brand: String,
    // None until the server has sent its command tree.
    pub commands: Option<(Vec<String>, Vec<String>, Option<String>)>,
}

// What the server's own command tree holds. Remote keeps only the chat's tree (the server's with the mods'
// commands merged in), so it is read where that tree is built; a new connection, which has no tree yet, clears it.
static COMMANDS: Mutex<Option<(Vec<String>, Vec<String>, Option<String>)>> = Mutex::new(None);

pub(crate) fn server_tree(tree: Option<&Dispatcher>) {
    *COMMANDS.lock().unwrap_or_else(|e| e.into_inner()) = tree.map(extract);
}

pub(crate) fn view(game: &Game<'_>) -> Option<View> {
    let remote = game.remote.as_ref()?;
    Some(View {
        address: remote.session.address.clone(),
        brand: remote.brand.chars().take(128).collect(),
        commands: COMMANDS.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    })
}

fn extract(tree: &Dispatcher) -> (Vec<String>, Vec<String>, Option<String>) {
    let mut roots = Vec::new();
    let mut literals = Vec::new();
    let mut suggestion = None;
    let mut queue = VecDeque::new();
    if let Some(root) = tree.nodes.first() {
        for &i in root.children.iter().take(512) {
            let Some(node) = tree.nodes.get(i) else {
                continue;
            };
            let NodeKind::Literal(root) = &node.kind else {
                continue;
            };
            if root.len() > 128 || !root.is_ascii() {
                continue;
            }
            roots.push(root.clone());
            if queue.len() < 512 {
                queue.push_back((i, root.clone(), 0u8));
            }
            // Only one low-rate request, on a visible version command whose argument explicitly
            // asks the server for suggestions (Bukkit's /version, aliases /ver and /about, completes
            // plugin names). Never guess hidden command names.
            if suggestion.is_none()
                && matches!(root.as_str(), "version" | "ver" | "about" | "bukkit:version" | "bukkit:ver" | "bukkit:about")
                && node
                    .children
                    .iter()
                    .filter_map(|&child| tree.nodes.get(child))
                    .any(|child| {
                        matches!(child.kind, NodeKind::Argument { .. })
                            && child.suggests == Some(Suggests::AskServer)
                    })
            {
                suggestion = Some(format!("/{root} "));
            }
        }
    }
    let mut visited = 0;
    while let Some((parent, path, depth)) = queue.pop_front() {
        visited += 1;
        if visited > 1024 || depth >= 4 || literals.len() >= 512 {
            break;
        }
        let Some(node) = tree.nodes.get(parent) else {
            continue;
        };
        for &child in &node.children {
            let Some(child_node) = tree.nodes.get(child) else {
                continue;
            };
            let (name, literal) = match &child_node.kind {
                NodeKind::Literal(name) => (name.as_str(), true),
                NodeKind::Argument { name, .. } => (name.as_str(), false),
                NodeKind::Root => continue,
            };
            if name.len() > 64 || !name.is_ascii() || path.len() + name.len() + 3 > 128 {
                continue;
            }
            let next = if literal {
                format!("{path} {name}")
            } else {
                format!("{path} <{name}>")
            };
            if literal {
                literals.push(next.clone());
            }
            if queue.len() < 512 {
                queue.push_back((child, next, depth + 1));
            }
            if literals.len() >= 512 {
                break;
            }
        }
    }
    (roots, literals, suggestion)
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_client::command::{Node, args::ArgType};

    #[test]
    fn extracts_only_server_tree_roots_and_visible_suggestion_path() {
        let mut tree = Dispatcher::new();
        tree.nodes.push(Node {
            kind: NodeKind::Literal("luckperms:lp".into()),
            children: vec![4],
            command: None,
            redirect: None,
            permission: 0,
            restricted: false,
            suggests: None,
        });
        tree.nodes.push(Node {
            kind: NodeKind::Literal("version".into()),
            children: vec![3],
            command: None,
            redirect: None,
            permission: 0,
            restricted: false,
            suggests: None,
        });
        tree.nodes.push(Node {
            kind: NodeKind::Argument {
                name: "plugin".into(),
                ty: ArgType::Word,
            },
            children: vec![],
            command: None,
            redirect: None,
            permission: 0,
            restricted: false,
            suggests: Some(Suggests::AskServer),
        });
        tree.nodes.push(Node {
            kind: NodeKind::Literal("user".into()),
            children: vec![],
            command: None,
            redirect: None,
            permission: 0,
            restricted: false,
            suggests: None,
        });
        tree.nodes[0].children = vec![1, 2, 999];
        assert_eq!(
            extract(&tree),
            (
                vec!["luckperms:lp".into(), "version".into()],
                vec!["luckperms:lp user".into()],
                Some("/version ".into())
            )
        );
        assert_eq!(extract(&Dispatcher::new()), (vec![], vec![], None));
    }
}
