//! Transactional path topology edits. Existing cubic segments retain their handles.
use crate::vector_path::{Point, Subpath, VectorPath};
use anyhow::{Context, Result, ensure};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub subpath: usize,
    pub last: bool,
}

fn reverse(path: &mut Subpath) {
    path.anchors.reverse();
    for anchor in &mut path.anchors {
        std::mem::swap(&mut anchor.incoming, &mut anchor.outgoing);
    }
}

/// Cut at an existing node. A closed contour opens at that node with coincident
/// endpoints; an open contour becomes two open contours. No curve is refitted.
pub fn split_at(path: &VectorPath, subpath: usize, anchor: usize) -> Result<VectorPath> {
    path.validate()?;
    let source = path.subpaths.get(subpath).context("Select a path")?;
    ensure!(anchor < source.anchors.len(), "Select a node to split");
    ensure!(source.anchors.len() >= 2, "A split needs a segment");
    let mut result = path.clone();
    if source.closed {
        let mut anchors = source.anchors[anchor..].to_vec();
        anchors.extend_from_slice(&source.anchors[..=anchor]);
        anchors.first_mut().unwrap().incoming = None;
        anchors.last_mut().unwrap().outgoing = None;
        result.subpaths[subpath] = Subpath {
            anchors,
            closed: false,
        };
    } else {
        ensure!(
            anchor > 0 && anchor + 1 < source.anchors.len(),
            "Select an interior node to split an open path"
        );
        let mut left = source.anchors[..=anchor].to_vec();
        let mut right = source.anchors[anchor..].to_vec();
        left.last_mut().unwrap().outgoing = None;
        right.first_mut().unwrap().incoming = None;
        result.subpaths.splice(
            subpath..=subpath,
            [
                Subpath {
                    anchors: left,
                    closed: false,
                },
                Subpath {
                    anchors: right,
                    closed: false,
                },
            ],
        );
    }
    result.validate()?;
    Ok(result)
}

fn endpoint(path: &VectorPath, end: Endpoint) -> Result<Point> {
    let sub = path
        .subpaths
        .get(end.subpath)
        .context("Path endpoint is missing")?;
    ensure!(!sub.closed, "A closed path has no endpoints");
    let anchor = if end.last {
        sub.anchors.last()
    } else {
        sub.anchors.first()
    };
    Ok(anchor.context("An empty path has no endpoints")?.position)
}

/// Join two explicit endpoints with a straight segment. Exactly coincident ends
/// collapse into one anchor while retaining the incoming and outgoing curves.
pub fn join(path: &VectorPath, first: Endpoint, second: Endpoint) -> Result<VectorPath> {
    path.validate()?;
    let a = endpoint(path, first)?;
    let b = endpoint(path, second)?;
    ensure!(first != second, "Choose two different endpoints");
    let mut result = path.clone();
    if first.subpath == second.subpath {
        let sub = &mut result.subpaths[first.subpath];
        ensure!(
            sub.anchors.len() >= 2,
            "A closed path needs at least two anchors"
        );
        if a == b && sub.anchors.len() > 2 {
            let last = sub.anchors.pop().unwrap();
            sub.anchors[0].incoming = last.incoming;
        } else {
            sub.anchors.first_mut().unwrap().incoming = None;
            sub.anchors.last_mut().unwrap().outgoing = None;
        }
        sub.closed = true;
    } else {
        let mut left = path.subpaths[first.subpath].clone();
        let mut right = path.subpaths[second.subpath].clone();
        if !first.last {
            reverse(&mut left);
        }
        if second.last {
            reverse(&mut right);
        }
        if a == b {
            left.anchors.last_mut().unwrap().outgoing = right.anchors[0].outgoing;
            left.anchors.extend(right.anchors.into_iter().skip(1));
        } else {
            left.anchors.last_mut().unwrap().outgoing = None;
            right.anchors[0].incoming = None;
            left.anchors.extend(right.anchors);
        }
        let keep = first.subpath.min(second.subpath);
        let remove = first.subpath.max(second.subpath);
        result.subpaths[keep] = left;
        result.subpaths.remove(remove);
    }
    result.validate()?;
    Ok(result)
}

/// Deterministic nearest endpoint within this compound path. The opposite end
/// of the same contour is included, allowing an explicit close operation.
pub fn nearest_endpoint(path: &VectorPath, from: Endpoint) -> Result<Endpoint> {
    path.validate()?;
    let origin = endpoint(path, from)?;
    let mut best: Option<(f64, Endpoint)> = None;
    for (index, sub) in path.subpaths.iter().enumerate() {
        if sub.closed || sub.anchors.is_empty() {
            continue;
        }
        for last in [false, true] {
            let candidate = Endpoint {
                subpath: index,
                last,
            };
            if candidate == from || (index == from.subpath && sub.anchors.len() < 2) {
                continue;
            }
            let point = endpoint(path, candidate)?;
            let distance =
                (point.x as f64 - origin.x as f64).hypot(point.y as f64 - origin.y as f64);
            if best.is_none_or(|(d, _)| distance < d) {
                best = Some((distance, candidate));
            }
        }
    }
    best.map(|(_, end)| end)
        .context("There is no other open endpoint to join")
}
