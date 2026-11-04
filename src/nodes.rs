//! Cheap project snapshots: only an edited node detaches from shared history.
use crate::model::Node;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::HashMap,
    ops::{Index, IndexMut, Range},
    sync::Arc,
};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Nodes {
    items: Vec<Arc<Node>>,
    #[serde(skip)]
    index: RefCell<Option<HashMap<String, usize>>>,
}
impl Clone for Nodes {
    fn clone(&self) -> Self {
        Self {
            items: self.items.clone(),
            index: RefCell::new(None),
        }
    }
}
impl PartialEq for Nodes {
    fn eq(&self, other: &Self) -> bool {
        self.items.len() == other.items.len()
            && self
                .items
                .iter()
                .zip(&other.items)
                .all(|(a, b)| Arc::ptr_eq(a, b) || **a == **b)
    }
}
fn node_ref(n: &Arc<Node>) -> &Node {
    n
}
fn node_mut(n: &mut Arc<Node>) -> &mut Node {
    Arc::make_mut(n)
}
pub type Iter<'a> = std::iter::Map<std::slice::Iter<'a, Arc<Node>>, fn(&Arc<Node>) -> &Node>;
pub type IterMut<'a> =
    std::iter::Map<std::slice::IterMut<'a, Arc<Node>>, fn(&mut Arc<Node>) -> &mut Node>;
impl Nodes {
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn iter(&self) -> Iter<'_> {
        self.items.iter().map(node_ref)
    }
    pub fn iter_mut(&mut self) -> IterMut<'_> {
        self.items.iter_mut().map(node_mut)
    }
    pub fn get(&self, i: usize) -> Option<&Node> {
        self.items.get(i).map(Arc::as_ref)
    }
    pub fn get_mut(&mut self, i: usize) -> Option<&mut Node> {
        self.items.get_mut(i).map(Arc::make_mut)
    }
    pub fn first(&self) -> Option<&Node> {
        self.get(0)
    }
    pub fn find(&self, id: &str) -> Option<&Node> {
        if let Some(i) = self
            .index
            .borrow()
            .as_ref()
            .and_then(|m| m.get(id))
            .copied()
            && let Some(node) = self.get(i).filter(|n| n.id == id)
        {
            return Some(node);
        }
        // IDs may be edited through a mutable node; verify cached entries before use.
        let mut cache = self.index.borrow_mut();
        if cache.is_none()
            || cache
                .as_ref()
                .is_some_and(|m| m.len() != self.len() || m.contains_key(id))
        {
            *cache = Some(
                self.iter()
                    .enumerate()
                    .map(|(i, n)| (n.id.clone(), i))
                    .collect(),
            );
        } else {
            // A miss can be an ID rename, so a scan is needed once on this path.
            if let Some((i, _)) = self.iter().enumerate().find(|(_, n)| n.id == id) {
                cache.as_mut().unwrap().insert(id.to_owned(), i);
            }
        }
        cache
            .as_ref()
            .and_then(|m| m.get(id))
            .and_then(|&i| self.get(i))
            .filter(|n| n.id == id)
    }
    pub fn push(&mut self, n: Node) {
        self.items.push(Arc::new(n));
        *self.index.get_mut() = None;
    }
    pub fn insert(&mut self, i: usize, n: Node) {
        self.items.insert(i, Arc::new(n));
        *self.index.get_mut() = None;
    }
    pub fn remove(&mut self, i: usize) -> Node {
        *self.index.get_mut() = None;
        Arc::unwrap_or_clone(self.items.remove(i))
    }
    pub fn swap(&mut self, a: usize, b: usize) {
        self.items.swap(a, b);
        *self.index.get_mut() = None;
    }
    pub fn splice(&mut self, range: Range<usize>, nodes: impl IntoIterator<Item = Node>) {
        self.items.splice(range, nodes.into_iter().map(Arc::new));
        *self.index.get_mut() = None;
    }
    pub fn shared_with(&self, other: &Self) -> usize {
        self.items
            .iter()
            .zip(&other.items)
            .filter(|(a, b)| Arc::ptr_eq(a, b))
            .count()
    }
}
impl From<Vec<Node>> for Nodes {
    fn from(nodes: Vec<Node>) -> Self {
        nodes.into_iter().collect()
    }
}
impl FromIterator<Node> for Nodes {
    fn from_iter<T: IntoIterator<Item = Node>>(iter: T) -> Self {
        Self {
            items: iter.into_iter().map(Arc::new).collect(),
            index: RefCell::new(None),
        }
    }
}
impl Index<usize> for Nodes {
    type Output = Node;
    fn index(&self, i: usize) -> &Node {
        &self.items[i]
    }
}
impl IndexMut<usize> for Nodes {
    fn index_mut(&mut self, i: usize) -> &mut Node {
        Arc::make_mut(&mut self.items[i])
    }
}
impl<'a> IntoIterator for &'a Nodes {
    type Item = &'a Node;
    type IntoIter = std::iter::Map<std::slice::Iter<'a, Arc<Node>>, fn(&Arc<Node>) -> &Node>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a> IntoIterator for &'a mut Nodes {
    type Item = &'a mut Node;
    type IntoIter =
        std::iter::Map<std::slice::IterMut<'a, Arc<Node>>, fn(&mut Arc<Node>) -> &mut Node>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
