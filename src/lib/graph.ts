export interface GraphInput {
  sha: string;
  parents: string[];
}

/** A line segment in lane units: x is a lane index, y is 0 (top) … 1 (bottom) of the row. */
export interface Seg {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  color: number;
}

export interface GraphRow {
  sha: string;
  lane: number;
  color: number;
  segs: Seg[];
  /** Number of lanes this row needs horizontally. */
  lanes: number;
}

/**
 * Lays out a topologically ordered commit list into lanes (children before parents).
 * Each lane holds the sha of the commit it is waiting for.
 */
export function computeGraph(commits: GraphInput[]): GraphRow[] {
  let lanes: (string | null)[] = [];
  const laneColors: number[] = [];
  let nextColor = 0;
  const rows: GraphRow[] = [];

  const firstFree = (): number => {
    const i = lanes.indexOf(null);
    return i >= 0 ? i : lanes.length;
  };

  for (const c of commits) {
    const before = [...lanes];
    const beforeColors = [...laneColors];

    let idx = lanes.indexOf(c.sha);
    if (idx < 0) {
      idx = firstFree();
      lanes[idx] = c.sha;
      laneColors[idx] = nextColor++;
    }
    const color = laneColors[idx];
    const segs: Seg[] = [];

    // Lanes that were waiting for this commit run into it from above.
    before.forEach((sha, j) => {
      if (sha === c.sha) segs.push({ x1: j, y1: 0, x2: idx, y2: 0.5, color: beforeColors[j] });
    });
    lanes.forEach((sha, j) => {
      if (sha === c.sha && j !== idx) lanes[j] = null;
    });

    if (c.parents.length === 0) {
      lanes[idx] = null;
    } else {
      lanes[idx] = c.parents[0];
      for (let k = 1; k < c.parents.length; k++) {
        const p = c.parents[k];
        let target = lanes.indexOf(p);
        if (target < 0) {
          target = firstFree();
          lanes[target] = p;
          laneColors[target] = nextColor++;
        }
        segs.push({ x1: idx, y1: 0.5, x2: target, y2: 1, color: laneColors[target] });
      }
      segs.push({ x1: idx, y1: 0.5, x2: idx, y2: 1, color });
    }

    // Lanes that merely pass through this row.
    before.forEach((sha, j) => {
      if (sha !== null && sha !== c.sha && j !== idx) {
        segs.push({ x1: j, y1: 0, x2: j, y2: 1, color: beforeColors[j] });
      }
    });

    while (lanes.length > 0 && lanes[lanes.length - 1] === null) lanes.pop();
    const width = Math.max(before.length, lanes.length, idx + 1, ...segs.map((s) => Math.max(s.x1, s.x2) + 1));
    rows.push({ sha: c.sha, lane: idx, color, segs, lanes: width });
  }
  return rows;
}

/**
 * Every commit reachable from `tip` (the tip itself and all its ancestors) among the loaded
 * commits. Used to highlight one branch's history in the graph.
 */
export function reachableFrom(commits: GraphInput[], tip: string): Set<string> {
  const byId = new Map(commits.map((c) => [c.sha, c]));
  const seen = new Set<string>();
  const stack = [tip];
  while (stack.length > 0) {
    const sha = stack.pop()!;
    if (seen.has(sha)) continue;
    const c = byId.get(sha);
    if (!c) continue; // beyond the loaded page
    seen.add(sha);
    stack.push(...c.parents);
  }
  return seen;
}
