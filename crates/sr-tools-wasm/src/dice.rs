//! Dice (UTL-DEV-001): RPG notation, evaluated in the browser.
//!
//! Grammar (whitespace is ignored, case-insensitive):
//!
//! ```text
//! expr     = term (('+' | '-') term)*
//! term     = factor ('*' factor)*
//! factor   = number | dice | '(' expr ')'
//! dice     = [count] 'd' sides modifier*        count defaults to 1
//! sides    = number | '%' (= 100) | 'F' (fudge: -1, 0, +1)
//! modifier = 'kh' [n] | 'kl' [n]                keep the n highest / lowest (n defaults to 1)
//!          | 'dh' [n] | 'dl' [n]                drop the n highest / lowest
//!          | '!' [cmp]                          explode (on the maximum, or where the comparison holds)
//!          | cmp                                count successes instead of summing
//! cmp      = ('>=' | '<=' | '>' | '<' | '=') number
//! ```
//!
//! The random source is a seeded `SplitMix64`: the browser passes 64 bits from `crypto.getRandomValues`,
//! so a roll is reproducible from its seed (tests, and "show the seed" in the UI) yet unpredictable in use.
//! Every size is bounded, so no input can run long or allocate much.

use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

/// Most dice in one group, most sides on one die, deepest explosion chain per die, and the total number of
/// individual rolls one expression may make. These keep the worst case in the low milliseconds.
pub const MAX_DICE: u32 = 1000;
pub const MAX_SIDES: u32 = 1_000_000;
pub const MAX_EXPLODE: u32 = 100;
pub const MAX_ROLLS: u32 = 20_000;
pub const MAX_EXPR_CHARS: usize = 200;
const MAX_DEPTH: u32 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiceError {
    Empty,
    TooLong,
    /// Unexpected character or token, with the character index where it was found.
    Syntax(usize),
    ZeroDice,
    TooManyDice,
    BadSides,
    /// A keep/drop count larger than the group, or an explosion that can never stop (`d1!`).
    BadModifier,
    TooManyRolls,
    TooDeep,
    Overflow,
}

impl std::fmt::Display for DiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty"),
            Self::TooLong => write!(f, "too-long"),
            Self::Syntax(i) => write!(f, "syntax:{i}"),
            Self::ZeroDice => write!(f, "zero-dice"),
            Self::TooManyDice => write!(f, "too-many-dice"),
            Self::BadSides => write!(f, "bad-sides"),
            Self::BadModifier => write!(f, "bad-modifier"),
            Self::TooManyRolls => write!(f, "too-many-rolls"),
            Self::TooDeep => write!(f, "too-deep"),
            Self::Overflow => write!(f, "overflow"),
        }
    }
}

/// SplitMix64: tiny, fast, well distributed, and fully determined by its seed.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in `0..n` (`n > 0`), without modulo bias (rejection sampling).
    pub fn below(&mut self, n: u64) -> u64 {
        debug_assert!(n > 0);
        let zone = u64::MAX - (u64::MAX % n);
        loop {
            let v = self.next_u64();
            if v < zone {
                return v % n;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cmp {
    Ge,
    Le,
    Gt,
    Lt,
    Eq,
}

impl Cmp {
    fn holds(self, v: i64, t: i64) -> bool {
        match self {
            Self::Ge => v >= t,
            Self::Le => v <= t,
            Self::Gt => v > t,
            Self::Lt => v < t,
            Self::Eq => v == t,
        }
    }
    fn sym(self) -> &'static str {
        match self {
            Self::Ge => ">=",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Lt => "<",
            Self::Eq => "=",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keep {
    High(u32),
    Low(u32),
    DropHigh(u32),
    DropLow(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Sides {
    N(u32),
    Fudge,
}

type Condition = (Cmp, i64);

#[derive(Debug, Clone)]
struct Group {
    count: u32,
    sides: Sides,
    keep: Option<Keep>,
    /// `Some(None)` explodes on the maximum face, `Some(Some(c))` where `c` holds.
    explode: Option<Option<Condition>>,
    success: Option<Condition>,
}

#[derive(Debug, Clone)]
enum Node {
    Num(i64),
    Dice(Group),
    Bin(Box<Node>, char, Box<Node>),
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn number(&mut self) -> Option<u32> {
        let start = self.i;
        let mut v: u64 = 0;
        while let Some(c @ b'0'..=b'9') = self.peek() {
            v = (v * 10 + u64::from(c - b'0')).min(u64::from(u32::MAX));
            self.i += 1;
        }
        (self.i > start).then_some(v as u32)
    }
    fn expr(&mut self, depth: u32) -> Result<Node, DiceError> {
        if depth > MAX_DEPTH {
            return Err(DiceError::TooDeep);
        }
        let mut left = self.term(depth)?;
        while let Some(op @ (b'+' | b'-')) = self.peek() {
            self.i += 1;
            let right = self.term(depth)?;
            left = Node::Bin(Box::new(left), op as char, Box::new(right));
        }
        Ok(left)
    }
    fn term(&mut self, depth: u32) -> Result<Node, DiceError> {
        let mut left = self.factor(depth)?;
        while self.eat(b'*') {
            let right = self.factor(depth)?;
            left = Node::Bin(Box::new(left), '*', Box::new(right));
        }
        Ok(left)
    }
    fn factor(&mut self, depth: u32) -> Result<Node, DiceError> {
        if self.eat(b'(') {
            let e = self.expr(depth + 1)?;
            if !self.eat(b')') {
                return Err(DiceError::Syntax(self.i));
            }
            return Ok(e);
        }
        let count = self.number();
        if self.peek() != Some(b'd') {
            return match count {
                Some(n) => Ok(Node::Num(i64::from(n))),
                None => Err(DiceError::Syntax(self.i)),
            };
        }
        self.i += 1;
        let count = count.unwrap_or(1);
        if count == 0 {
            return Err(DiceError::ZeroDice);
        }
        if count > MAX_DICE {
            return Err(DiceError::TooManyDice);
        }
        let sides = if self.eat(b'%') {
            Sides::N(100)
        } else if self.eat(b'f') {
            Sides::Fudge
        } else {
            match self.number() {
                Some(n) if (1..=MAX_SIDES).contains(&n) => Sides::N(n),
                Some(_) => return Err(DiceError::BadSides),
                None => return Err(DiceError::Syntax(self.i)),
            }
        };
        let mut g = Group {
            count,
            sides,
            keep: None,
            explode: None,
            success: None,
        };
        loop {
            let at = self.i;
            if self.eat(b'!') {
                if g.explode.is_some() {
                    return Err(DiceError::Syntax(at));
                }
                let cmp = self.cmp()?;
                g.explode = Some(cmp);
            } else if let Some(c) = self.cmp()? {
                if g.success.is_some() {
                    return Err(DiceError::Syntax(at));
                }
                g.success = Some(c);
            } else if let Some(k) = self.keep() {
                if g.keep.is_some() {
                    return Err(DiceError::Syntax(at));
                }
                g.keep = Some(k);
            } else {
                break;
            }
        }
        validate(&g)?;
        Ok(Node::Dice(g))
    }
    fn cmp(&mut self) -> Result<Option<Condition>, DiceError> {
        let op = match (self.peek(), self.s.get(self.i + 1).copied()) {
            (Some(b'>'), Some(b'=')) => {
                self.i += 2;
                Cmp::Ge
            }
            (Some(b'<'), Some(b'=')) => {
                self.i += 2;
                Cmp::Le
            }
            (Some(b'>'), _) => {
                self.i += 1;
                Cmp::Gt
            }
            (Some(b'<'), _) => {
                self.i += 1;
                Cmp::Lt
            }
            (Some(b'='), _) => {
                self.i += 1;
                Cmp::Eq
            }
            _ => return Ok(None),
        };
        let neg = self.eat(b'-');
        let n = self.number().ok_or(DiceError::Syntax(self.i))?;
        Ok(Some((op, if neg { -i64::from(n) } else { i64::from(n) })))
    }
    fn keep(&mut self) -> Option<Keep> {
        let kind: fn(u32) -> Keep = match (self.peek(), self.s.get(self.i + 1).copied()) {
            (Some(b'k'), Some(b'h')) => Keep::High,
            (Some(b'k'), Some(b'l')) => Keep::Low,
            (Some(b'd'), Some(b'h')) => Keep::DropHigh,
            (Some(b'd'), Some(b'l')) => Keep::DropLow,
            _ => return None,
        };
        self.i += 2;
        Some(kind(self.number().unwrap_or(1)))
    }
}

fn validate(g: &Group) -> Result<(), DiceError> {
    if let Some(Keep::High(n) | Keep::Low(n) | Keep::DropHigh(n) | Keep::DropLow(n)) = g.keep
        && n > g.count
    {
        return Err(DiceError::BadModifier);
    }
    if let Some(cond) = g.explode {
        let (lo, hi) = bounds(&g.sides);
        // An explosion that holds for every face never stops.
        let always = match cond {
            None => lo == hi,
            Some((c, t)) => (lo..=hi).all(|v| c.holds(v, t)),
        };
        if always {
            return Err(DiceError::BadModifier);
        }
    }
    Ok(())
}

fn bounds(s: &Sides) -> (i64, i64) {
    match s {
        Sides::N(n) => (1, i64::from(*n)),
        Sides::Fudge => (-1, 1),
    }
}

/// One die as it was rolled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Die {
    pub value: i64,
    /// Dropped by keep/drop: shown struck through, not counted.
    pub dropped: bool,
    /// Added by an explosion.
    pub exploded: bool,
    /// Counted as a success (success-counting groups only).
    pub success: bool,
}

/// One dice group of the expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupResult {
    pub notation: String,
    pub dice: Vec<Die>,
    pub total: i64,
}

/// The outcome of one expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollResult {
    pub expression: String,
    pub total: i64,
    pub groups: Vec<GroupResult>,
    /// The expression with each dice group replaced by its total, e.g. `8 + 2`.
    pub breakdown: String,
}

impl RollResult {
    pub fn to_json(&self) -> Value {
        let groups: Vec<Value> = self
            .groups
            .iter()
            .map(|g| {
                json!({
                    "notation": g.notation,
                    "total": g.total,
                    "dice": g.dice.iter().map(|d| json!({
                        "value": d.value,
                        "dropped": d.dropped,
                        "exploded": d.exploded,
                        "success": d.success,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({
            "expression": self.expression,
            "total": self.total,
            "breakdown": self.breakdown,
            "groups": groups,
        })
    }
}

struct Ctx<'a> {
    rng: &'a mut Rng,
    rolls: u32,
    groups: Vec<GroupResult>,
}

fn notation(g: &Group) -> String {
    let sides = match g.sides {
        Sides::N(n) => n.to_string(),
        Sides::Fudge => "F".into(),
    };
    let mut s = format!("{}d{sides}", g.count);
    match g.keep {
        Some(Keep::High(n)) => s.push_str(&format!("kh{n}")),
        Some(Keep::Low(n)) => s.push_str(&format!("kl{n}")),
        Some(Keep::DropHigh(n)) => s.push_str(&format!("dh{n}")),
        Some(Keep::DropLow(n)) => s.push_str(&format!("dl{n}")),
        None => {}
    }
    match g.explode {
        Some(None) => s.push('!'),
        Some(Some((c, t))) => s.push_str(&format!("!{}{t}", c.sym())),
        None => {}
    }
    if let Some((c, t)) = g.success {
        s.push_str(&format!("{}{t}", c.sym()));
    }
    s
}

fn roll_die(ctx: &mut Ctx, sides: &Sides) -> Result<i64, DiceError> {
    ctx.rolls += 1;
    if ctx.rolls > MAX_ROLLS {
        return Err(DiceError::TooManyRolls);
    }
    Ok(match sides {
        Sides::N(n) => ctx.rng.below(u64::from(*n)) as i64 + 1,
        Sides::Fudge => ctx.rng.below(3) as i64 - 1,
    })
}

fn roll_group(ctx: &mut Ctx, g: &Group) -> Result<i64, DiceError> {
    let (_, hi) = bounds(&g.sides);
    let mut dice: Vec<Die> = Vec::new();
    for _ in 0..g.count {
        let mut v = roll_die(ctx, &g.sides)?;
        dice.push(Die {
            value: v,
            dropped: false,
            exploded: false,
            success: false,
        });
        if let Some(cond) = g.explode {
            let mut depth = 0;
            while cond.map_or(v == hi, |(c, t)| c.holds(v, t)) && depth < MAX_EXPLODE {
                v = roll_die(ctx, &g.sides)?;
                dice.push(Die {
                    value: v,
                    dropped: false,
                    exploded: true,
                    success: false,
                });
                depth += 1;
            }
        }
    }
    if let Some(k) = g.keep {
        // Order indices by value, then mark the ones that do not survive.
        let mut idx: Vec<usize> = (0..dice.len()).collect();
        idx.sort_by_key(|&i| (dice[i].value, i));
        let n = dice.len();
        let drop: Vec<usize> = match k {
            Keep::High(m) => idx[..n.saturating_sub(m as usize)].to_vec(),
            Keep::Low(m) => idx[(m as usize).min(n)..].to_vec(),
            Keep::DropHigh(m) => idx[n.saturating_sub(m as usize)..].to_vec(),
            Keep::DropLow(m) => idx[..(m as usize).min(n)].to_vec(),
        };
        for i in drop {
            dice[i].dropped = true;
        }
    }
    let mut total = 0i64;
    for d in dice.iter_mut().filter(|d| !d.dropped) {
        match g.success {
            Some((c, t)) => {
                d.success = c.holds(d.value, t);
                total += i64::from(d.success);
            }
            None => total = total.checked_add(d.value).ok_or(DiceError::Overflow)?,
        }
    }
    ctx.groups.push(GroupResult {
        notation: notation(g),
        dice,
        total,
    });
    Ok(total)
}

fn eval(ctx: &mut Ctx, n: &Node, out: &mut String) -> Result<i64, DiceError> {
    match n {
        Node::Num(v) => {
            out.push_str(&v.to_string());
            Ok(*v)
        }
        Node::Dice(g) => {
            let v = roll_group(ctx, g)?;
            out.push_str(&v.to_string());
            Ok(v)
        }
        Node::Bin(l, op, r) => {
            let a = eval(ctx, l, out)?;
            out.push_str(&format!(" {op} "));
            let b = eval(ctx, r, out)?;
            match op {
                '+' => a.checked_add(b),
                '-' => a.checked_sub(b),
                _ => a.checked_mul(b),
            }
            .ok_or(DiceError::Overflow)
        }
    }
}

/// Parse and roll `input` with `rng`.
pub fn roll_expr(input: &str, rng: &mut Rng) -> Result<RollResult, DiceError> {
    let cleaned: String = input
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if cleaned.is_empty() {
        return Err(DiceError::Empty);
    }
    if let Some(i) = cleaned.chars().position(|c| !c.is_ascii()) {
        return Err(DiceError::Syntax(i));
    }
    if cleaned.len() > MAX_EXPR_CHARS {
        return Err(DiceError::TooLong);
    }
    let mut p = Parser {
        s: cleaned.as_bytes(),
        i: 0,
    };
    let node = p.expr(0)?;
    if p.i != cleaned.len() {
        return Err(DiceError::Syntax(p.i));
    }
    let mut ctx = Ctx {
        rng,
        rolls: 0,
        groups: Vec::new(),
    };
    let mut breakdown = String::new();
    let total = eval(&mut ctx, &node, &mut breakdown)?;
    Ok(RollResult {
        expression: cleaned,
        total,
        groups: ctx.groups,
        breakdown,
    })
}

/// Flip a coin: `true` is heads.
pub fn flip(rng: &mut Rng) -> bool {
    rng.below(2) == 0
}

/// A weighted pick: the index chosen among `weights` (all weights zero picks nothing).
pub fn pick_weighted(weights: &[u32], rng: &mut Rng) -> Option<usize> {
    let sum: u64 = weights.iter().map(|w| u64::from(*w)).sum();
    if sum == 0 {
        return None;
    }
    let mut t = rng.below(sum);
    for (i, w) in weights.iter().enumerate() {
        let w = u64::from(*w);
        if t < w {
            return Some(i);
        }
        t -= w;
    }
    None
}

fn seed_of(hi: u32, lo: u32) -> Rng {
    Rng::new((u64::from(hi) << 32) | u64::from(lo))
}

/// Roll `expression`; the seed is two 32-bit halves so no `BigInt` crosses the boundary. Returns the
/// [`RollResult`] as JSON. Throws the error's code (`syntax:7`, `too-many-dice`, ...).
#[wasm_bindgen(js_name = diceRoll)]
pub fn dice_roll(expression: &str, seed_hi: u32, seed_lo: u32) -> Result<String, JsError> {
    let mut rng = seed_of(seed_hi, seed_lo);
    let r = roll_expr(expression, &mut rng).map_err(|e| JsError::new(&e.to_string()))?;
    Ok(r.to_json().to_string())
}

/// A coin flip: `true` is heads.
#[wasm_bindgen(js_name = diceFlip)]
pub fn dice_flip(seed_hi: u32, seed_lo: u32) -> bool {
    flip(&mut seed_of(seed_hi, seed_lo))
}

/// A weighted pick among `weights`; returns the index, or -1 when every weight is zero.
#[wasm_bindgen(js_name = dicePickWeighted)]
pub fn dice_pick_weighted(weights: &[u32], seed_hi: u32, seed_lo: u32) -> i32 {
    pick_weighted(weights, &mut seed_of(seed_hi, seed_lo)).map_or(-1, |i| i as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roll(s: &str, seed: u64) -> Result<RollResult, DiceError> {
        roll_expr(s, &mut Rng::new(seed))
    }

    #[test]
    fn same_seed_same_result() {
        assert_eq!(roll("4d6dl1+2", 7), roll("4d6dl1+2", 7));
        assert_ne!(
            roll("10d100", 1).unwrap().total,
            roll("10d100", 2).unwrap().total
        );
    }

    #[test]
    fn plain_numbers_and_arithmetic() {
        assert_eq!(roll("7", 1).unwrap().total, 7);
        assert_eq!(roll("1+2*3", 1).unwrap().total, 7);
        assert_eq!(roll("(1+2)*3", 1).unwrap().total, 9);
        assert_eq!(roll("10-3-2", 1).unwrap().total, 5);
        assert_eq!(roll(" 2 * ( 3 + 4 ) ", 1).unwrap().total, 14);
    }

    #[test]
    fn every_total_is_in_range_over_many_seeds() {
        for seed in 0..300u64 {
            let r = roll("2d20kh1+5", seed).unwrap();
            assert!((6..=25).contains(&r.total), "{}", r.total);
            let r = roll("4d6dl1", seed).unwrap();
            assert!((3..=18).contains(&r.total));
            let r = roll("d%", seed).unwrap();
            assert!((1..=100).contains(&r.total));
            let r = roll("4dF", seed).unwrap();
            assert!((-4..=4).contains(&r.total));
            let r = roll("d1", seed).unwrap();
            assert_eq!(r.total, 1);
        }
    }

    #[test]
    fn keep_and_drop_mark_the_right_dice() {
        let r = roll("4d6dl1", 3).unwrap();
        let g = &r.groups[0];
        assert_eq!(g.dice.len(), 4);
        assert_eq!(g.dice.iter().filter(|d| d.dropped).count(), 1);
        let min = g.dice.iter().map(|d| d.value).min().unwrap();
        assert_eq!(g.dice.iter().find(|d| d.dropped).unwrap().value, min);
        let kept: i64 = g.dice.iter().filter(|d| !d.dropped).map(|d| d.value).sum();
        assert_eq!(kept, r.total);
        // kh2 keeps the two highest.
        let r = roll("5d10kh2", 9).unwrap();
        let mut vals: Vec<i64> = r.groups[0].dice.iter().map(|d| d.value).collect();
        vals.sort();
        assert_eq!(r.total, vals[3] + vals[4]);
        // kl keeps the lowest.
        let r = roll("5d10kl2", 9).unwrap();
        let mut vals: Vec<i64> = r.groups[0].dice.iter().map(|d| d.value).collect();
        vals.sort();
        assert_eq!(r.total, vals[0] + vals[1]);
        // dh drops the highest.
        let r = roll("3d6dh1", 4).unwrap();
        let mut vals: Vec<i64> = r.groups[0].dice.iter().map(|d| d.value).collect();
        vals.sort();
        assert_eq!(r.total, vals[0] + vals[1]);
    }

    #[test]
    fn exploding_dice_add_dice_while_they_roll_the_max() {
        let mut saw_explosion = false;
        for seed in 0..200 {
            let r = roll("3d4!", seed).unwrap();
            let g = &r.groups[0];
            if g.dice.iter().any(|d| d.exploded) {
                saw_explosion = true;
            }
            // Every exploded die follows a die that rolled the maximum.
            for (i, d) in g.dice.iter().enumerate() {
                if d.exploded {
                    assert_eq!(g.dice[i - 1].value, 4);
                }
            }
            assert_eq!(r.total, g.dice.iter().map(|d| d.value).sum::<i64>());
        }
        assert!(saw_explosion);
        // A comparison form: explode on 5 or more.
        let r = roll("2d6!>=5", 11).unwrap();
        assert_eq!(r.groups[0].notation, "2d6!>=5");
    }

    #[test]
    fn success_counting() {
        for seed in 0..50 {
            let r = roll("6d10>=8", seed).unwrap();
            let g = &r.groups[0];
            let n = g.dice.iter().filter(|d| d.value >= 8).count() as i64;
            assert_eq!(r.total, n);
            assert!(g.dice.iter().all(|d| d.success == (d.value >= 8)));
        }
        let r = roll("10d6=6", 5).unwrap();
        assert!((0..=10).contains(&r.total));
        let r = roll("10d6<2", 5).unwrap();
        assert_eq!(
            r.total,
            r.groups[0].dice.iter().filter(|d| d.value == 1).count() as i64
        );
    }

    #[test]
    fn combinations_and_case() {
        let r = roll("2D6 + 1d4 + 3", 2).unwrap();
        assert_eq!(r.groups.len(), 2);
        assert_eq!(r.expression, "2d6+1d4+3");
        assert!(r.breakdown.ends_with(" + 3"));
        let r = roll("2d6*2", 2).unwrap();
        assert_eq!(r.total % 2, 0);
        // Keep with success counting in one group.
        assert!(roll("5d10kh3>=6", 2).is_ok());
    }

    #[test]
    fn rejects_bad_input_with_a_code() {
        assert_eq!(roll("", 1), Err(DiceError::Empty));
        assert_eq!(roll("   ", 1), Err(DiceError::Empty));
        assert_eq!(roll("0d6", 1), Err(DiceError::ZeroDice));
        assert_eq!(roll("1001d6", 1), Err(DiceError::TooManyDice));
        assert_eq!(roll("d0", 1), Err(DiceError::BadSides));
        assert_eq!(roll("d1000001", 1), Err(DiceError::BadSides));
        assert_eq!(roll("4d6kh5", 1), Err(DiceError::BadModifier));
        assert_eq!(roll("d1!", 1), Err(DiceError::BadModifier));
        assert_eq!(roll("d6!>=1", 1), Err(DiceError::BadModifier));
        assert!(matches!(roll("d", 1), Err(DiceError::Syntax(_))));
        assert!(matches!(roll("2d6+", 1), Err(DiceError::Syntax(_))));
        assert!(matches!(roll("2d6)", 1), Err(DiceError::Syntax(_))));
        assert!(matches!(roll("(2d6", 1), Err(DiceError::Syntax(_))));
        assert!(matches!(roll("hello", 1), Err(DiceError::Syntax(_))));
        assert!(matches!(roll("2d6!!", 1), Err(DiceError::Syntax(_))));
        assert!(matches!(roll("2d6>3>4", 1), Err(DiceError::Syntax(_))));
        assert!(matches!(roll("2d\u{e9}", 1), Err(DiceError::Syntax(_))));
        assert_eq!(roll(&"1+".repeat(200), 1), Err(DiceError::TooLong));
        assert_eq!(roll(&"(".repeat(50), 1), Err(DiceError::TooDeep));
    }

    #[test]
    fn the_total_roll_budget_stops_runaway_expressions() {
        let e = vec!["1000d2!"; 21].join("+");
        assert!(e.len() <= MAX_EXPR_CHARS);
        assert_eq!(roll(&e, 1), Err(DiceError::TooManyRolls));
        // Overflow is an error, not a wrap.
        assert_eq!(
            roll("4294967295*4294967295*4294967295", 1),
            Err(DiceError::Overflow)
        );
    }

    #[test]
    fn the_distribution_is_roughly_uniform() {
        let mut rng = Rng::new(42);
        let mut counts = [0u32; 6];
        for _ in 0..60_000 {
            counts[(rng.below(6)) as usize] += 1;
        }
        for c in counts {
            assert!((9_000..11_000).contains(&c), "{c}");
        }
    }

    #[test]
    fn coin_and_weighted_pick() {
        let mut rng = Rng::new(5);
        let heads = (0..2000).filter(|_| flip(&mut rng)).count();
        assert!((800..1200).contains(&heads));
        let mut rng = Rng::new(6);
        let mut hits = [0u32; 3];
        for _ in 0..30_000 {
            hits[pick_weighted(&[1, 0, 3], &mut rng).unwrap()] += 1;
        }
        assert_eq!(hits[1], 0);
        assert!(hits[2] > hits[0] * 2);
        assert_eq!(pick_weighted(&[0, 0], &mut rng), None);
        assert_eq!(pick_weighted(&[], &mut rng), None);
    }

    #[test]
    fn notation_is_normalised() {
        assert_eq!(roll("d20", 1).unwrap().groups[0].notation, "1d20");
        assert_eq!(roll("D%", 1).unwrap().groups[0].notation, "1d100");
        assert_eq!(roll("4d6dl", 1).unwrap().groups[0].notation, "4d6dl1");
        assert_eq!(roll("2d20kh", 1).unwrap().groups[0].notation, "2d20kh1");
        assert_eq!(roll("3d6!", 1).unwrap().groups[0].notation, "3d6!");
        assert_eq!(roll("4df", 1).unwrap().groups[0].notation, "4dF");
    }

    #[test]
    fn json_shape() {
        let v = roll("2d6+1", 3).unwrap().to_json();
        assert_eq!(v["groups"][0]["dice"].as_array().unwrap().len(), 2);
        assert!(v["total"].is_i64());
        assert_eq!(v["expression"], "2d6+1");
    }

    #[test]
    fn no_input_panics() {
        let alphabet = b"0123456789dDkKhHlL!<>=+-*()%fF .";
        let mut rng = Rng::new(99);
        for _ in 0..3000 {
            let len = rng.below(14) as usize;
            let s: String = (0..len)
                .map(|_| alphabet[rng.below(alphabet.len() as u64) as usize] as char)
                .collect();
            let _ = roll_expr(&s, &mut rng);
        }
    }
}
