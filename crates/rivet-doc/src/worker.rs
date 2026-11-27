//! The thread every document lives on. PDFium can't be called from two threads at once, so one
//! thread opens, draws, reads and searches every document, and the window sends it requests
//! and gets answers back, never waiting for one.
//!
//! What it does first: opening and closing, then the text of pages, then drawing the pages the
//! window shows, then thumbnails, then one more page of a search. Drawings asked for a page
//! the window has since scrolled away from are dropped before they start.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::dark::{DarkLook, Table};
use crate::{Area, Bitmap, Document, Info, Kind, Link, OpenError, OutlineItem, PageText};

pub type DocId = u64;

/// A drawing to make.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub doc: DocId,
    pub page: usize,
    /// The whole page's size in pixels.
    pub width: u32,
    pub height: u32,
    /// Only this part of it, in those pixels: [left, top, right, bottom].
    pub clip: Option<[u32; 4]>,
    pub dark: Option<DarkLook>,
    /// For the thumbnails: drawn last, and never dropped for being off screen.
    pub thumb: bool,
}

pub enum Request {
    Open { doc: DocId, path: PathBuf, password: Option<String> },
    Close { doc: DocId },
    /// The pages the window shows now, with those just above and below.
    Showing { doc: DocId, pages: HashSet<usize> },
    Render(Job),
    /// A page's text and links.
    Text { doc: DocId, page: usize },
    /// Search the whole document, from `from` round to the page before it. An empty query
    /// stops the search under way.
    Find { doc: DocId, query: String, match_case: bool, from: usize, serial: u64 },
}

/// A document, opened.
pub struct Opened {
    pub kind: Kind,
    pub sizes: Vec<[f32; 2]>,
    pub outline: Vec<OutlineItem>,
    pub info: Info,
}

pub enum Reply {
    Opened { doc: DocId, result: Result<Opened, OpenError> },
    Rendered { job: Job, bitmap: Result<Bitmap, String> },
    Text { doc: DocId, page: usize, text: Option<PageText>, links: Vec<Link> },
    /// What a search found on one page: each match's boxes. `searched` pages are done of the
    /// document's; `done` when it has been round them all.
    Found { doc: DocId, serial: u64, page: usize, hits: Vec<Vec<Area>>, searched: usize, done: bool },
}

pub struct Worker {
    tx: Sender<Request>,
    rx: Receiver<Reply>,
}

struct Search {
    doc: DocId,
    query: String,
    match_case: bool,
    serial: u64,
    from: usize,
    searched: usize,
}

impl Worker {
    /// Start the thread. `wake` is called after each answer, to have the window look.
    pub fn start(wake: impl Fn() + Send + 'static) -> Worker {
        let (tx, requests) = channel();
        let (answers, rx) = channel();
        std::thread::Builder::new().name("rivet-doc".into()).spawn(move || run(requests, answers, wake)).expect("a thread for documents");
        Worker { tx, rx }
    }

    pub fn send(&self, r: Request) {
        let _ = self.tx.send(r);
    }

    pub fn try_recv(&self) -> Option<Reply> {
        self.rx.try_recv().ok()
    }
}

fn run(requests: Receiver<Request>, answers: Sender<Reply>, wake: impl Fn()) {
    let mut docs: HashMap<DocId, Box<dyn Document>> = HashMap::new();
    let mut sizes: HashMap<DocId, Vec<[f32; 2]>> = HashMap::new();
    let mut showing: HashMap<DocId, HashSet<usize>> = HashMap::new();
    let mut queue: VecDeque<Request> = VecDeque::new();
    let mut search: Option<Search> = None;
    let mut table: Option<Table> = None;
    let send = |r: Reply| {
        let ok = answers.send(r).is_ok();
        wake();
        ok
    };
    loop {
        // Wait when there's nothing to do; then take everything that has come in.
        if queue.is_empty() && search.is_none() {
            match requests.recv() {
                Ok(r) => queue.push_back(r),
                Err(_) => return,
            }
        }
        while let Ok(r) = requests.try_recv() {
            queue.push_back(r);
        }
        let mut rest = VecDeque::with_capacity(queue.len());
        for r in queue.drain(..) {
            match r {
                Request::Open { doc, path, password } => {
                    let result = crate::open(&path, password.as_deref()).map(|mut d| {
                        let opened = Opened { kind: d.kind(), sizes: d.page_sizes(), outline: d.outline(), info: d.info() };
                        sizes.insert(doc, opened.sizes.clone());
                        docs.insert(doc, d);
                        opened
                    });
                    if !send(Reply::Opened { doc, result }) {
                        return;
                    }
                }
                Request::Close { doc } => {
                    docs.remove(&doc);
                    sizes.remove(&doc);
                    showing.remove(&doc);
                    if search.as_ref().is_some_and(|s| s.doc == doc) {
                        search = None;
                    }
                }
                Request::Showing { doc, pages } => {
                    showing.insert(doc, pages);
                }
                Request::Find { doc, query, match_case, from, serial } => {
                    search = (!query.trim().is_empty() && docs.contains_key(&doc)).then_some(Search { doc, query, match_case, serial, from, searched: 0 });
                }
                other => {
                    if !rest.contains_eq(&other) {
                        rest.push_back(other);
                    }
                }
            }
        }
        queue = rest;
        // Drawings for pages no longer shown are dropped; the rest go in order: text, pages,
        // thumbnails.
        queue.retain(|r| match r {
            Request::Render(j) => docs.contains_key(&j.doc) && (j.thumb || showing.get(&j.doc).is_none_or(|s| s.contains(&j.page))),
            Request::Text { doc, .. } => docs.contains_key(doc),
            _ => true,
        });
        let next = queue
            .iter()
            .position(|r| matches!(r, Request::Text { .. }))
            .or_else(|| queue.iter().position(|r| matches!(r, Request::Render(j) if !j.thumb)))
            .or_else(|| (!queue.is_empty()).then_some(0));
        if let Some(i) = next {
            let r = queue.remove(i).expect("present");
            let reply = match r {
                Request::Text { doc, page } => {
                    let d = docs.get_mut(&doc).expect("checked");
                    Reply::Text { doc, page, text: d.text(page), links: d.links(page) }
                }
                Request::Render(job) => {
                    let d = docs.get_mut(&job.doc).expect("checked");
                    let mut bitmap = d.render(job.page, job.width, job.height, job.clip);
                    if let (Ok(bmp), Some(look), Kind::Pdf) = (&mut bitmap, job.dark, d.kind()) {
                        if table.as_ref().is_none_or(|t| t.look() != look) {
                            table = Some(Table::new(look));
                        }
                        let pictures = d.pictures(job.page);
                        let page_pt = sizes.get(&job.doc).and_then(|s| s.get(job.page)).copied().unwrap_or([1.0, 1.0]);
                        let origin = job.clip.map_or([0, 0], |c| [c[0], c[1]]);
                        if let Some(t) = &table {
                            t.apply(bmp, [job.width, job.height], origin, page_pt, &pictures);
                        }
                    }
                    Reply::Rendered { job, bitmap }
                }
                _ => continue,
            };
            if !send(reply) {
                return;
            }
            continue;
        }
        // Nothing else waiting: one more page of the search.
        if let Some(s) = search.as_mut() {
            let Some(d) = docs.get_mut(&s.doc) else {
                search = None;
                continue;
            };
            let n = d.page_count();
            let page = (s.from + s.searched) % n.max(1);
            let hits = d.text(page).map(|t| crate::find::matches(&t, &s.query, s.match_case).into_iter().map(|(a, b)| t.areas(a, b)).collect()).unwrap_or_default();
            s.searched += 1;
            let done = s.searched >= n;
            let reply = Reply::Found { doc: s.doc, serial: s.serial, page, hits, searched: s.searched, done };
            if done {
                search = None;
            }
            if !send(reply) {
                return;
            }
        }
    }
}

trait ContainsEq {
    fn contains_eq(&self, r: &Request) -> bool;
}

impl ContainsEq for VecDeque<Request> {
    /// The same drawing, or the same page's text, already asked for.
    fn contains_eq(&self, r: &Request) -> bool {
        self.iter().any(|q| match (q, r) {
            (Request::Render(a), Request::Render(b)) => a == b,
            (Request::Text { doc: a, page: p }, Request::Text { doc: b, page: q }) => a == b && p == q,
            _ => false,
        })
    }
}
