//! Wiki pages as tree nodes for the left column.
//!
//! The tree has one root, "Wiki", whose leaves are the pages. Page
//! order is the wiki's own order (sorted by title); the tree itself adds
//! no sorting, so what the user sees is what `list_pages` returns.

use mia_wiki::PageMeta;
use sequoia_ui::view::{TreeNode, TreeView};

/// Build the wiki tree from the pages `list_pages` returned.
pub fn pages_to_nodes(pages: &[PageMeta]) -> Vec<TreeNode<PageMeta>> {
    let leaves = pages
        .iter()
        .map(|page| TreeNode::leaf(page.id.to_string(), page.clone()))
        .collect();
    vec![TreeNode::branch("wiki", wiki_root_meta(), leaves)]
}

/// The synthetic root entry: it is a folder, not a page.
fn wiki_root_meta() -> PageMeta {
    PageMeta {
        id: mia_wiki::PageId::new("wiki"),
        title: "Wiki".to_string(),
    }
}

/// Build a `TreeView` over the pages in `pages`.
pub fn pages_tree(pages: &sequoia_ui::reactive::Signal<Vec<PageMeta>>) -> TreeView<PageMeta> {
    let pages = pages.clone();
    TreeView::new(move || pages_to_nodes(&pages.get()))
        .label("Wiki")
        .row_height(28.0)
        .indent(16.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(id: &str, title: &str) -> PageMeta {
        PageMeta {
            id: mia_wiki::PageId::new(id),
            title: title.to_string(),
        }
    }

    #[test]
    fn empty_pages_give_one_empty_root() {
        let nodes = pages_to_nodes(&[]);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].value.title, "Wiki");
        assert!(nodes[0].children.is_empty());
    }

    #[test]
    fn pages_become_leaves_under_the_root() {
        let pages = vec![meta("alpha", "Progetto Alpha"), meta("beta", "Beta")];
        let nodes = pages_to_nodes(&pages);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].children.len(), 2);
        assert_eq!(nodes[0].children[0].id, "alpha");
        assert_eq!(nodes[0].children[0].value.title, "Progetto Alpha");
        assert_eq!(nodes[0].children[1].id, "beta");
        assert!(nodes[0].children[0].children.is_empty());
    }
}
