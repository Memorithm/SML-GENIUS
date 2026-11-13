//! SYNTH-LEARN1C: active version-space search for native Boolean gates
//! implementing posit8 sign subfunctions (not complete ADD/MUL).
//! Candidate labels are acquired only through selected membership queries.
#![forbid(unsafe_code)]
#[path = "posit8_circuit_synth.rs"]
#[allow(dead_code)]
mod oracle;
use oracle::Posit8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Add,
    Mul,
}
impl Operation {
    fn name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Mul => "mul",
        }
    }
    fn eligible(self, a: u8, b: u8) -> bool {
        if a == 0x80 || b == 0x80 {
            return false;
        }
        match self {
            Self::Add => a == 0 || b == 0 || (a >> 7) == (b >> 7),
            Self::Mul => a != 0 && b != 0,
        }
    }
    fn teacher(self, a: u8, b: u8) -> u8 {
        assert!(self.eligible(a, b), "outside declared subdomain");
        let output = match self {
            Self::Add => Posit8(a).add(Posit8(b)).0,
            Self::Mul => Posit8(a).mul(Posit8(b)).0,
        };
        (output >> 7) & 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    Zero,
    One,
    SignA,
    SignB,
    NotA,
    NotB,
    And,
    Or,
    Xor,
}
impl Gate {
    fn candidates() -> [Self; 9] {
        [
            Self::Zero,
            Self::One,
            Self::SignA,
            Self::SignB,
            Self::NotA,
            Self::NotB,
            Self::And,
            Self::Or,
            Self::Xor,
        ]
    }
    pub fn evaluate(self, a: u8, b: u8) -> u8 {
        let sa = (a >> 7) & 1;
        let sb = (b >> 7) & 1;
        match self {
            Self::Zero => 0,
            Self::One => 1,
            Self::SignA => sa,
            Self::SignB => sb,
            Self::NotA => sa ^ 1,
            Self::NotB => sb ^ 1,
            Self::And => sa & sb,
            Self::Or => sa | sb,
            Self::Xor => sa ^ sb,
        }
    }
    fn cost(self) -> usize {
        match self {
            Self::Zero | Self::One | Self::SignA | Self::SignB => 0,
            _ => 1,
        }
    }
    /// The emitted x86-64 function yields a sign MASK (0 or 0x80),
    /// not a full posit8 ADD/MUL result.
    pub fn emit_gas(self, operation: Operation) -> String {
        let symbol = format!("mem_posit8_learned_{}_sign", operation.name());
        let gate_instruction = match self {
            Self::Zero => "xor eax, eax",
            Self::One => "mov eax, 1",
            Self::SignA => "nop",
            Self::SignB => "mov eax, edx",
            Self::NotA => "xor eax, 1",
            Self::NotB => "mov eax, edx\n    xor eax, 1",
            Self::And => "and eax, edx",
            Self::Or => "or eax, edx",
            Self::Xor => "xor eax, edx",
        };
        format!(
            ".intel_syntax noprefix\n.text\n.globl {symbol}\n.type {symbol}, @function\n{symbol}:\n    movzx eax, dil\n    shr eax, 7\n    movzx edx, sil\n    shr edx, 7\n    {gate_instruction}\n    shl eax, 7\n    ret\n.size {symbol}, .-{symbol}\n.section .note.GNU-stack,\"\",@progbits\n"
        )
    }
}

const ADD_POOL: [(u8, u8); 16] = [
    (0, 0),
    (1, 1),
    (0, 0xff),
    (0xff, 0),
    (0xff, 0xff),
    (0x40, 0x40),
    (0x7f, 1),
    (0, 0x81),
    (0x81, 0),
    (0x81, 0x81),
    (0xc0, 0xc0),
    (0xff, 0xc0),
    (0x40, 0),
    (0, 0x40),
    (0x7f, 0x7f),
    (1, 0),
];
const MUL_POOL: [(u8, u8); 16] = [
    (1, 1),
    (1, 0x81),
    (0x81, 1),
    (0x81, 0x81),
    (0x40, 0x7f),
    (0x40, 0xc0),
    (0xc0, 0x40),
    (0xc0, 0xc0),
    (0xff, 0xff),
    (0xff, 0x7f),
    (0x7f, 0xff),
    (0x7f, 0x7f),
    (1, 0xff),
    (0xff, 1),
    (0x81, 0x7f),
    (0x7f, 0x81),
];
#[derive(Debug)]
pub struct Discovery {
    pub operation: Operation,
    pub chosen_gate: Gate,
    pub teacher_queries: Vec<(u8, u8, u8)>,
    pub eligible_count: usize,
    pub unseen_errors: usize,
}
pub fn discover(operation: Operation) -> Discovery {
    let pool = match operation {
        Operation::Add => &ADD_POOL,
        Operation::Mul => &MUL_POOL,
    };
    assert!(pool.iter().all(|&(a, b)| operation.eligible(a, b)));
    let mut remaining = Gate::candidates().to_vec();
    let mut observations = Vec::<(u8, u8, u8)>::new();
    loop {
        // Choose the maximum-disagreement unlabeled point WITHOUT asking the
        // teacher for labels of any other candidate point.
        let choice = pool
            .iter()
            .copied()
            .filter(|&(a, b)| !observations.iter().any(|&(x, y, _)| x == a && y == b))
            .map(|(a, b)| {
                let ones = remaining
                    .iter()
                    .filter(|&&candidate| candidate.evaluate(a, b) != 0)
                    .count();
                let zeros = remaining.len() - ones;
                (ones * zeros, a, b)
            })
            .max_by_key(|(score, _, _)| *score);
        let Some((disagreement, a, b)) = choice else {
            break;
        };
        if disagreement == 0 {
            break;
        }
        let expected = operation.teacher(a, b);
        observations.push((a, b, expected));
        remaining.retain(|gate| gate.evaluate(a, b) == expected);
        assert!(
            !remaining.is_empty(),
            "candidate grammar cannot fit teacher"
        );
    }
    assert!(!remaining.is_empty() && observations.len() <= pool.len());
    remaining.sort_by_key(|gate| gate.cost());
    let chosen_gate = remaining[0];

    // Audit only: none of these unqueried labels enters the search above.
    let mut eligible_count = 0usize;
    let mut unseen_errors = 0usize;
    for a in 0..=u8::MAX {
        for b in 0..=u8::MAX {
            if !operation.eligible(a, b) {
                continue;
            }
            eligible_count += 1;
            if !observations.iter().any(|&(x, y, _)| x == a && y == b)
                && chosen_gate.evaluate(a, b) != operation.teacher(a, b)
            {
                unseen_errors += 1;
            }
        }
    }
    Discovery {
        operation,
        chosen_gate,
        teacher_queries: observations,
        eligible_count,
        unseen_errors,
    }
}

fn main() {
    let arg = std::env::args().nth(1);
    let operations = match arg.as_deref() {
        Some("--emit-x86-add-sign") => vec![Operation::Add],
        Some("--emit-x86-mul-sign") => vec![Operation::Mul],
        None => vec![Operation::Add, Operation::Mul],
        _ => panic!("use --emit-x86-add-sign, --emit-x86-mul-sign, or no argument"),
    };
    for operation in operations {
        let learned = discover(operation);
        if arg.is_some() {
            print!("{}", learned.chosen_gate.emit_gas(operation));
        } else {
            println!(
                "SYNTH_LEARN1C,op={},gate={:?},teacher_queries={},eligible={},unseen_errors={},exact_subdomain={}",
                operation.name(),
                learned.chosen_gate,
                learned.teacher_queries.len(),
                learned.eligible_count,
                learned.unseen_errors,
                learned.unseen_errors == 0
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_sign_gate_is_discovered_from_membership_queries() {
        let result = discover(Operation::Add);
        assert_eq!(result.chosen_gate, Gate::Or);
        assert!(result.teacher_queries.len() <= 16);
        assert_eq!(result.unseen_errors, 0);
        assert!(result.eligible_count > 30_000);
        assert!(result
            .teacher_queries
            .iter()
            .all(|&(a, b, sign)| sign == result.chosen_gate.evaluate(a, b)));
    }
    #[test]
    fn multiplication_sign_gate_is_discovered_from_membership_queries() {
        let result = discover(Operation::Mul);
        assert_eq!(result.chosen_gate, Gate::Xor);
        assert!(result.teacher_queries.len() <= 16);
        assert_eq!(result.unseen_errors, 0);
        assert_eq!(result.eligible_count, 254 * 254);
    }
    #[test]
    fn native_generator_exposes_only_sign_mask() {
        for operation in [Operation::Add, Operation::Mul] {
            let found = discover(operation);
            let gas = found.chosen_gate.emit_gas(operation);
            assert!(gas.contains(".intel_syntax noprefix"));
            assert!(gas.contains("    shl eax, 7"));
            assert!(gas.contains(".note.GNU-stack"));
            assert!(!gas.contains(".rodata"));
        }
    }
}
