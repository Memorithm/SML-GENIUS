//! PGF2-3B K=2048 block-local locality-rescue ablation.
//!
//! Frozen protocol: docs/SML-PGF2-3B-BLOCK-LOCAL-PREREGISTRATION.md.
//! The PGF2-3 fixtures and controls are reproduced without modification.

use std::collections::BTreeSet;

use sml_core::{
    pascal_gf2::pascal_gf2_in_place,
    semantic::{bind, inverse_rotate_permute, rotate_permute, unbind, SemanticBitField},
};

const BITS: usize = 2048;
const WORDS: usize = BITS / 64;
const CLASSES: usize = 32;
const ROUTES: usize = 8;
const QUERY_SEEDS: u64 = 16;
const PERMUTE_SHIFT: usize = 683;
const PPM: u64 = 1_000_000;
const ARMS: usize = 8;

const BASE: u64 = 0x5047_4632_5342_4733;
const CLASS_TAG: u64 = 0x9e37_79b9_7f4a_7c15;
const ROUTE_TAG: u64 = 0xd6e8_feb8_6659_fd93;
const QUERY_TAG: u64 = 0xc2b2_ae3d_27d4_eb4f;
const NOISE_TAG: u64 = 0x1656_67b1_9e37_79f9;

const UNIFORM_H: [usize; 5] = [0, 1, 4, 16, 64];
const BOUNDARY: [(&str, usize, usize); 3] = [("1_4", 1, 4), ("1_2", 1, 2), ("3_4", 3, 4)];

type Field = SemanticBitField<BITS>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arm {
    Identity,
    Permute,
    XorPermute,
    Pgf2Global,
    Pgf2B64,
    Pgf2B128,
    Pgf2B256,
    Pgf2B512,
}

impl Arm {
    const ALL: [Self; ARMS] = [
        Self::Identity,
        Self::Permute,
        Self::XorPermute,
        Self::Pgf2Global,
        Self::Pgf2B64,
        Self::Pgf2B128,
        Self::Pgf2B256,
        Self::Pgf2B512,
    ];

    const CONTROLS: [Self; 3] = [Self::Identity, Self::Permute, Self::XorPermute];

    const CANDIDATES: [Self; 4] = [
        Self::Pgf2B64,
        Self::Pgf2B128,
        Self::Pgf2B256,
        Self::Pgf2B512,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Identity => 0,
            Self::Permute => 1,
            Self::XorPermute => 2,
            Self::Pgf2Global => 3,
            Self::Pgf2B64 => 4,
            Self::Pgf2B128 => 5,
            Self::Pgf2B256 => 6,
            Self::Pgf2B512 => 7,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Identity => "IDENTITY",
            Self::Permute => "PERMUTE",
            Self::XorPermute => "XOR_PERMUTE",
            Self::Pgf2Global => "PGF2_GLOBAL",
            Self::Pgf2B64 => "PGF2_B64",
            Self::Pgf2B128 => "PGF2_B128",
            Self::Pgf2B256 => "PGF2_B256",
            Self::Pgf2B512 => "PGF2_B512",
        }
    }

    const fn block_bits(self) -> Option<usize> {
        match self {
            Self::Pgf2Global => Some(BITS),
            Self::Pgf2B64 => Some(64),
            Self::Pgf2B128 => Some(128),
            Self::Pgf2B256 => Some(256),
            Self::Pgf2B512 => Some(512),
            Self::Identity | Self::Permute | Self::XorPermute => None,
        }
    }
}

#[derive(Debug, Clone)]
struct FixtureFamily {
    name: &'static str,
    family_id: u64,
    classes: Vec<Field>,
    routes: Vec<Field>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Rate {
    observations: u64,
    correct: u64,
    ties: u64,
}

impl Rate {
    const fn ppm(self) -> u64 {
        if self.observations == 0 {
            0
        } else {
            self.correct * PPM / self.observations
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Separation {
    min: u64,
    median: u64,
    sum: u64,
    count: u64,
}

impl Separation {
    const fn mean_ppm(self) -> u64 {
        if self.count == 0 {
            0
        } else {
            self.sum * PPM / (self.count * BITS as u64)
        }
    }

    const fn median_ppm(self) -> u64 {
        self.median * PPM / BITS as u64
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Stability {
    min: u64,
    median: u64,
    sum: u64,
    count: u64,
    p95: u64,
    max: u64,
}

impl Stability {
    const fn mean_ppm_of_width(self) -> u64 {
        if self.count == 0 {
            0
        } else {
            self.sum * PPM / (self.count * BITS as u64)
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Cost {
    logical_xor_ops: u64,
    packed_word_updates: u64,
    state_bytes: usize,
    scratch_words: usize,
}

#[derive(Debug, Clone, Copy)]
struct Stream(u64);

impl Stream {
    const fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

const fn derived_seed(family: u64, kind: u64, id: u64, sample: u64) -> u64 {
    BASE ^ (family + 1).wrapping_mul(CLASS_TAG)
        ^ (kind + 1).wrapping_mul(ROUTE_TAG)
        ^ (id + 1).wrapping_mul(QUERY_TAG)
        ^ (sample + 1).wrapping_mul(NOISE_TAG)
}

fn random_field(seed: u64) -> Field {
    let mut stream = Stream::new(seed);
    let words = (0..WORDS).map(|_| stream.next()).collect::<Vec<_>>();
    Field::from_words(words).expect("K=2048 random field")
}

fn toggle(field: &mut Field, index: usize) {
    let current = field.get(index).expect("toggle read");
    field.set(index, !current).expect("toggle write");
}

fn structured_family() -> FixtureFamily {
    let anchor = random_field(derived_seed(0, 0, 0, 0));

    let classes = (0..CLASSES)
        .map(|class| {
            let mut field = anchor.clone();
            for offset in 0..16 {
                toggle(&mut field, 32 * class + offset);
            }
            field
        })
        .collect();

    let routes = (0..ROUTES)
        .map(|route| {
            let mut field = anchor.clone();
            for offset in 0..32 {
                toggle(&mut field, 1024 + 64 * route + offset);
            }
            field
        })
        .collect();

    FixtureFamily {
        name: "STRUCTURED_LOCAL",
        family_id: 0,
        classes,
        routes,
    }
}

fn balanced_random_family() -> FixtureFamily {
    let classes = (0..CLASSES)
        .map(|class| random_field(derived_seed(1, 1, class as u64, 0)))
        .collect();
    let routes = (0..ROUTES)
        .map(|route| random_field(derived_seed(1, 2, route as u64, 0)))
        .collect();

    FixtureFamily {
        name: "BALANCED_RANDOM",
        family_id: 1,
        classes,
        routes,
    }
}

fn fixed_role() -> Field {
    let indices = (0..64)
        .map(|index| (31 + 73 * index) % BITS)
        .collect::<Vec<_>>();
    Field::from_set_indices(&indices).expect("fixed role")
}

fn pgf2_block_local(input: &Field, block_bits: usize) -> Field {
    assert!(block_bits.is_power_of_two());
    assert!((64..=BITS).contains(&block_bits));
    assert_eq!(BITS % block_bits, 0);

    let words_per_block = block_bits / 64;
    let mut words = input.words().to_vec();
    for block in words.chunks_exact_mut(words_per_block) {
        let stats = pascal_gf2_in_place(block).expect("block-local PGF2");
        assert_eq!(stats.bits, block_bits);
        assert_eq!(stats.words, words_per_block);
    }
    Field::from_words(words).expect("block-local PGF2 field")
}

fn transform(arm: Arm, input: &Field, role: &Field) -> Field {
    match arm {
        Arm::Identity => input.clone(),
        Arm::Permute => rotate_permute(input, PERMUTE_SHIFT).expect("fixed permutation"),
        Arm::XorPermute => bind(role, input).expect("qualified bind"),
        Arm::Pgf2Global
        | Arm::Pgf2B64
        | Arm::Pgf2B128
        | Arm::Pgf2B256
        | Arm::Pgf2B512 => {
            pgf2_block_local(input, arm.block_bits().expect("PGF2 block width"))
        }
    }
}

fn recover(arm: Arm, transformed: &Field, role: &Field) -> Field {
    match arm {
        Arm::Identity => transformed.clone(),
        Arm::Permute => {
            inverse_rotate_permute(transformed, PERMUTE_SHIFT).expect("inverse permutation")
        }
        Arm::XorPermute => unbind(role, transformed).expect("qualified unbind"),
        Arm::Pgf2Global
        | Arm::Pgf2B64
        | Arm::Pgf2B128
        | Arm::Pgf2B256
        | Arm::Pgf2B512 => transform(arm, transformed, role),
    }
}

fn transform_all(arm: Arm, inputs: &[Field], role: &Field) -> Vec<Field> {
    inputs
        .iter()
        .map(|input| transform(arm, input, role))
        .collect()
}

fn shuffled_coordinates(family: u64, kind: u64, id: u64, sample: u64) -> Vec<usize> {
    let mut coordinates = (0..BITS).collect::<Vec<_>>();
    let mut stream = Stream::new(derived_seed(family, kind, id, sample));
    for index in (1..BITS).rev() {
        let other = stream.below(index + 1);
        coordinates.swap(index, other);
    }
    coordinates
}

fn uniform_query(
    base: &Field,
    family: u64,
    kind: u64,
    id: u64,
    sample: u64,
    flips: usize,
) -> Field {
    if flips == 0 {
        return base.clone();
    }
    let coordinates = shuffled_coordinates(family, kind, id, sample);
    let mut query = base.clone();
    for coordinate in coordinates.into_iter().take(flips) {
        toggle(&mut query, coordinate);
    }
    query
}

fn differing_coordinates(left: &Field, right: &Field) -> Vec<usize> {
    (0..BITS)
        .filter(|&index| left.get(index).expect("left") != right.get(index).expect("right"))
        .collect()
}

fn boundary_query(
    target: &Field,
    confounder: &Field,
    query_seed: usize,
    numerator: usize,
    denominator: usize,
) -> Field {
    let differences = differing_coordinates(target, confounder);
    assert!(!differences.is_empty());
    let offset = query_seed % differences.len();
    let take = differences.len() * numerator / denominator;
    let mut query = target.clone();
    for step in 0..take {
        let coordinate = differences[(offset + step) % differences.len()];
        toggle(&mut query, coordinate);
    }
    query
}

fn nearest_unique(query: &Field, prototypes: &[Field]) -> (Option<usize>, u64, bool) {
    let mut best_distance = u64::MAX;
    let mut best_index = None;
    let mut tied = false;

    for (index, prototype) in prototypes.iter().enumerate() {
        let distance = query.xor_popcount(prototype);
        if distance < best_distance {
            best_distance = distance;
            best_index = Some(index);
            tied = false;
        } else if distance == best_distance {
            tied = true;
        }
    }

    if tied {
        (None, best_distance, true)
    } else {
        (best_index, best_distance, false)
    }
}

fn separation(prototypes: &[Field]) -> Separation {
    let mut distances = Vec::with_capacity(prototypes.len());
    for (index, prototype) in prototypes.iter().enumerate() {
        let nearest = prototypes
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(_, other)| prototype.xor_popcount(other))
            .min()
            .expect("at least two prototypes");
        distances.push(nearest);
    }
    distances.sort_unstable();

    let count = distances.len();
    let median = if count % 2 == 0 {
        (distances[count / 2 - 1] + distances[count / 2]) / 2
    } else {
        distances[count / 2]
    };

    Separation {
        min: distances[0],
        median,
        sum: distances.iter().sum(),
        count: count as u64,
    }
}

fn collision_excess(prototypes: &[Field]) -> u64 {
    let unique = prototypes
        .iter()
        .map(|field| field.words().to_vec())
        .collect::<BTreeSet<_>>()
        .len();
    (prototypes.len() - unique) as u64
}

fn low12_collision_excess(prototypes: &[Field]) -> u64 {
    let unique = prototypes
        .iter()
        .map(|field| field.words()[0] & 0x0fff)
        .collect::<BTreeSet<_>>()
        .len();
    (prototypes.len() - unique) as u64
}

fn uniform_retrieval(
    family: &FixtureFamily,
    arm: Arm,
    transformed_prototypes: &[Field],
    role: &Field,
    h_index: usize,
) -> Rate {
    let flips = UNIFORM_H[h_index];
    let mut rate = Rate::default();

    for (class, base) in family.classes.iter().enumerate() {
        for seed in 0..QUERY_SEEDS {
            let query = uniform_query(
                base,
                family.family_id,
                3 + h_index as u64,
                class as u64,
                seed,
                flips,
            );
            let transformed = transform(arm, &query, role);
            let (nearest, _, tied) = nearest_unique(&transformed, transformed_prototypes);
            rate.observations += 1;
            rate.correct += u64::from(nearest == Some(class));
            rate.ties += u64::from(tied);
        }
    }

    rate
}

fn uniform_routing(
    family: &FixtureFamily,
    arm: Arm,
    transformed_routes: &[Field],
    role: &Field,
    h_index: usize,
) -> Rate {
    let flips = UNIFORM_H[h_index];
    let mut rate = Rate::default();

    for (route, base) in family.routes.iter().enumerate() {
        for seed in 0..QUERY_SEEDS {
            let query = uniform_query(
                base,
                family.family_id,
                8 + h_index as u64,
                route as u64,
                seed,
                flips,
            );
            let transformed = transform(arm, &query, role);
            let (nearest, _, tied) = nearest_unique(&transformed, transformed_routes);
            rate.observations += 1;
            rate.correct += u64::from(nearest == Some(route));
            rate.ties += u64::from(tied);
        }
    }

    rate
}

fn boundary_retrieval(
    family: &FixtureFamily,
    arm: Arm,
    transformed_prototypes: &[Field],
    role: &Field,
    numerator: usize,
    denominator: usize,
) -> Rate {
    let mut rate = Rate::default();

    for (class, target) in family.classes.iter().enumerate() {
        let confounder = class ^ 1;
        for seed in 0..QUERY_SEEDS as usize {
            let query = boundary_query(
                target,
                &family.classes[confounder],
                seed,
                numerator,
                denominator,
            );
            let transformed = transform(arm, &query, role);
            let (nearest, _, tied) = nearest_unique(&transformed, transformed_prototypes);
            rate.observations += 1;
            rate.correct += u64::from(nearest == Some(class));
            rate.ties += u64::from(tied);
        }
    }

    rate
}

fn stability(arm: Arm, role: &Field) -> Stability {
    let zero = Field::zeroed().expect("zero");
    let transformed_zero = transform(arm, &zero, role);
    let mut distances = Vec::with_capacity(BITS);

    for coordinate in 0..BITS {
        let basis = Field::from_set_indices(&[coordinate]).expect("basis");
        let transformed = transform(arm, &basis, role);
        distances.push(transformed_zero.xor_popcount(&transformed));
    }

    distances.sort_unstable();
    let median = (distances[BITS / 2 - 1] + distances[BITS / 2]) / 2;
    let p95_rank = (95 * BITS).div_ceil(100);
    Stability {
        min: distances[0],
        median,
        sum: distances.iter().sum(),
        count: BITS as u64,
        p95: distances[p95_rank - 1],
        max: distances[BITS - 1],
    }
}

fn cost(arm: Arm) -> Cost {
    match arm {
        Arm::Identity => Cost {
            state_bytes: 256,
            ..Cost::default()
        },
        Arm::Permute => Cost {
            state_bytes: 256,
            ..Cost::default()
        },
        Arm::XorPermute => Cost {
            logical_xor_ops: 2048,
            packed_word_updates: 32,
            state_bytes: 256,
            scratch_words: 0,
        },
        Arm::Pgf2Global
        | Arm::Pgf2B64
        | Arm::Pgf2B128
        | Arm::Pgf2B256
        | Arm::Pgf2B512 => {
            let block_bits = arm.block_bits().expect("block width");
            let words_per_block = block_bits / 64;
            let mut words = vec![0_u64; WORDS];
            let mut result = Cost {
                state_bytes: 256,
                ..Cost::default()
            };
            for block in words.chunks_exact_mut(words_per_block) {
                let stats = pascal_gf2_in_place(block).expect("cost transform");
                result.logical_xor_ops += stats.logical_xor_ops;
                result.packed_word_updates += stats.packed_word_updates;
                result.scratch_words += stats.scratch_words;
            }
            result
        }
    }
}

fn best_control_rate(rates: &[Rate; ARMS]) -> u64 {
    Arm::CONTROLS
        .iter()
        .map(|arm| rates[arm.index()].ppm())
        .max()
        .expect("controls")
}

fn best_control_median(separations: &[Separation; ARMS]) -> u64 {
    Arm::CONTROLS
        .iter()
        .map(|arm| separations[arm.index()].median)
        .max()
        .expect("controls")
}

fn min_control_collision(values: &[u64; ARMS]) -> u64 {
    Arm::CONTROLS
        .iter()
        .map(|arm| values[arm.index()])
        .min()
        .expect("controls")
}

fn within_ppm(candidate: u64, reference: u64, tolerance: u64) -> bool {
    candidate.saturating_add(tolerance) >= reference
}

fn main() {
    let role = fixed_role();
    let families = [structured_family(), balanced_random_family()];

    let mut class_separation = [[Separation::default(); ARMS]; 2];
    let mut route_separation = [[Separation::default(); ARMS]; 2];
    let mut class_collisions = [[0_u64; ARMS]; 2];
    let mut route_collisions = [[0_u64; ARMS]; 2];
    let mut low12_collisions = [[0_u64; ARMS]; 2];
    let mut retrieval = [[[Rate::default(); ARMS]; 5]; 2];
    let mut routing = [[[Rate::default(); ARMS]; 5]; 2];
    let mut boundary = [[[Rate::default(); ARMS]; 3]; 2];
    let mut inverse_checks = [0_u64; ARMS];
    let mut inverse_failures = [0_u64; ARMS];

    for (family_index, family) in families.iter().enumerate() {
        for arm in Arm::ALL {
            let arm_index = arm.index();
            let transformed_classes = transform_all(arm, &family.classes, &role);
            let transformed_routes = transform_all(arm, &family.routes, &role);

            for (source, transformed) in family
                .classes
                .iter()
                .chain(&family.routes)
                .zip(transformed_classes.iter().chain(&transformed_routes))
            {
                inverse_checks[arm_index] += 1;
                inverse_failures[arm_index] +=
                    u64::from(recover(arm, transformed, &role) != *source);
            }

            class_separation[family_index][arm_index] = separation(&transformed_classes);
            route_separation[family_index][arm_index] = separation(&transformed_routes);
            class_collisions[family_index][arm_index] = collision_excess(&transformed_classes);
            route_collisions[family_index][arm_index] = collision_excess(&transformed_routes);
            low12_collisions[family_index][arm_index] =
                low12_collision_excess(&transformed_classes);

            let class_sep = class_separation[family_index][arm_index];
            let route_sep = route_separation[family_index][arm_index];
            println!(
                "PGF2_3B_SEPARATION,family={},arm={},target=CLASS,min={},median={},mean_num={},mean_den={},mean_ppm={}",
                family.name,
                arm.name(),
                class_sep.min,
                class_sep.median,
                class_sep.sum,
                class_sep.count,
                class_sep.mean_ppm(),
            );
            println!(
                "PGF2_3B_SEPARATION,family={},arm={},target=ROUTE,min={},median={},mean_num={},mean_den={},mean_ppm={}",
                family.name,
                arm.name(),
                route_sep.min,
                route_sep.median,
                route_sep.sum,
                route_sep.count,
                route_sep.mean_ppm(),
            );
            println!(
                "PGF2_3B_COLLISION,family={},arm={},class_full_excess={},route_full_excess={},class_low12_excess={}",
                family.name,
                arm.name(),
                class_collisions[family_index][arm_index],
                route_collisions[family_index][arm_index],
                low12_collisions[family_index][arm_index],
            );

            for (h_index, flips) in UNIFORM_H.iter().copied().enumerate() {
                let class_rate =
                    uniform_retrieval(family, arm, &transformed_classes, &role, h_index);
                let route_rate = uniform_routing(family, arm, &transformed_routes, &role, h_index);
                retrieval[family_index][h_index][arm_index] = class_rate;
                routing[family_index][h_index][arm_index] = route_rate;

                println!(
                    "PGF2_3B_RETRIEVAL,family={},arm={},panel=UNIFORM,h={},observations={},correct={},ties={},ppm={}",
                    family.name,
                    arm.name(),
                    flips,
                    class_rate.observations,
                    class_rate.correct,
                    class_rate.ties,
                    class_rate.ppm(),
                );
                println!(
                    "PGF2_3B_ROUTING,family={},arm={},panel=UNIFORM,h={},observations={},correct={},ties={},ppm={}",
                    family.name,
                    arm.name(),
                    flips,
                    route_rate.observations,
                    route_rate.correct,
                    route_rate.ties,
                    route_rate.ppm(),
                );
            }

            for (boundary_index, (label, numerator, denominator)) in
                BOUNDARY.iter().copied().enumerate()
            {
                let rate = boundary_retrieval(
                    family,
                    arm,
                    &transformed_classes,
                    &role,
                    numerator,
                    denominator,
                );
                boundary[family_index][boundary_index][arm_index] = rate;
                println!(
                    "PGF2_3B_RETRIEVAL,family={},arm={},panel=BOUNDARY,fraction={},observations={},correct={},ties={},ppm={}",
                    family.name,
                    arm.name(),
                    label,
                    rate.observations,
                    rate.correct,
                    rate.ties,
                    rate.ppm(),
                );
            }
        }
    }

    let mut stability_by_arm = [Stability::default(); ARMS];
    let mut costs = [Cost::default(); ARMS];

    for arm in Arm::ALL {
        let arm_index = arm.index();
        let evidence = stability(arm, &role);
        stability_by_arm[arm_index] = evidence;
        println!(
            "PGF2_3B_STABILITY,arm={},observations={},min={},median={},mean_num={},mean_den={},mean_ppm_of_width={},p95={},max={}",
            arm.name(),
            evidence.count,
            evidence.min,
            evidence.median,
            evidence.sum,
            evidence.count,
            evidence.mean_ppm_of_width(),
            evidence.p95,
            evidence.max,
        );

        let arm_cost = cost(arm);
        costs[arm_index] = arm_cost;
        let coordinate_maps = match arm {
            Arm::Permute | Arm::XorPermute => BITS,
            _ => 0,
        };
        println!(
            "PGF2_3B_COST,arm={},state_bytes={},output_bytes=256,scratch_bytes={},coordinate_maps={},logical_xor_ops={},packed_word_updates={}",
            arm.name(),
            arm_cost.state_bytes,
            arm_cost.scratch_words * core::mem::size_of::<u64>(),
            coordinate_maps,
            arm_cost.logical_xor_ops,
            arm_cost.packed_word_updates,
        );
        println!(
            "PGF2_3B_INVERSE,arm={},checks={},failures={}",
            arm.name(),
            inverse_checks[arm_index],
            inverse_failures[arm_index],
        );
    }

    let controls_are_isometric = (0..2).all(|family| {
        let first = Arm::Identity.index();
        Arm::CONTROLS.iter().all(|arm| {
            let index = arm.index();
            class_separation[family][index].min == class_separation[family][first].min
                && class_separation[family][index].median
                    == class_separation[family][first].median
                && route_separation[family][index].min == route_separation[family][first].min
                && route_separation[family][index].median
                    == route_separation[family][first].median
        })
    });
    println!(
        "PGF2_3B_CONTROL,matched_control_isometry={}",
        u8::from(controls_are_isometric),
    );

    let h1 = Arm::CANDIDATES.iter().all(|arm| {
        let index = arm.index();
        inverse_failures[index] == 0
            && (0..2).all(|family| {
                class_collisions[family][index] <= min_control_collision(&class_collisions[family])
                    && route_collisions[family][index]
                        <= min_control_collision(&route_collisions[family])
            })
    });

    let h6 = Arm::CANDIDATES.iter().all(|arm| {
        let index = arm.index();
        let random_class_control = best_control_median(&class_separation[1]);
        let random_route_control = best_control_median(&route_separation[1]);
        class_separation[1][index]
            .median_ppm()
            .abs_diff(random_class_control * PPM / BITS as u64)
            <= 20_000
            && route_separation[1][index]
                .median_ppm()
                .abs_diff(random_route_control * PPM / BITS as u64)
                <= 20_000
            && retrieval[1][0][index].ppm() == best_control_rate(&retrieval[1][0])
            && routing[1][0][index].ppm() == best_control_rate(&routing[1][0])
    });

    let structured_class_control = best_control_median(&class_separation[0]);
    let structured_route_control = best_control_median(&route_separation[0]);
    let global_index = Arm::Pgf2Global.index();

    let mut selected = None;
    let mut any_h2 = false;
    let mut any_h3 = false;
    let mut any_h4 = false;
    let mut any_h5 = false;
    let mut any_h7 = false;
    let mut any_h8 = false;

    for arm in Arm::CANDIDATES {
        let index = arm.index();

        let h2 = class_separation[0][index].median.saturating_mul(10)
            >= structured_class_control.saturating_mul(9)
            && route_separation[0][index].median.saturating_mul(10)
                >= structured_route_control.saturating_mul(9);

        let h3 = [1_usize, 2].iter().all(|&h_index| {
            within_ppm(
                retrieval[0][h_index][index].ppm(),
                best_control_rate(&retrieval[0][h_index]),
                10_000,
            ) && within_ppm(
                routing[0][h_index][index].ppm(),
                best_control_rate(&routing[0][h_index]),
                10_000,
            )
        });

        let h4 = within_ppm(
            retrieval[0][3][index].ppm(),
            best_control_rate(&retrieval[0][3]),
            50_000,
        ) && within_ppm(
            routing[0][3][index].ppm(),
            best_control_rate(&routing[0][3]),
            50_000,
        );

        let h5 = retrieval[0][4][index].ppm()
            >= retrieval[0][4][global_index].ppm().saturating_add(100_000)
            && routing[0][4][index].ppm()
                >= routing[0][4][global_index].ppm().saturating_add(100_000);

        let stability = stability_by_arm[index];
        let h7 = stability.sum <= 32 * stability.count
            && stability.p95 <= 64
            && stability.max <= arm.block_bits().expect("candidate block width") as u64;

        let candidate_cost = costs[index];
        let h8 = candidate_cost.logical_xor_ops < 11_264
            && candidate_cost.packed_word_updates < 272
            && candidate_cost.state_bytes == 256
            && candidate_cost.scratch_words == 0;

        any_h2 |= h2;
        any_h3 |= h3;
        any_h4 |= h4;
        any_h5 |= h5;
        any_h7 |= h7;
        any_h8 |= h8;

        let candidate_pass = h1 && h6 && h2 && h3 && h4 && h5 && h7 && h8;
        println!(
            "PGF2_3B_CANDIDATE_GATE,arm={},H2={},H3={},H4={},H5={},H7={},H8={},ALL={}",
            arm.name(),
            if h2 { "HOLDS" } else { "REFUTED" },
            if h3 { "HOLDS" } else { "REFUTED" },
            if h4 { "HOLDS" } else { "REFUTED" },
            if h5 { "HOLDS" } else { "REFUTED" },
            if h7 { "HOLDS" } else { "REFUTED" },
            if h8 { "HOLDS" } else { "REFUTED" },
            if candidate_pass { "HOLDS" } else { "REFUTED" },
        );

        if selected.is_none() && candidate_pass {
            selected = Some(arm);
        }
    }

    for (name, holds) in [
        ("H1_EXACTNESS", h1),
        ("H2_STRUCTURED_GEOMETRY_RESCUE_ANY", any_h2),
        ("H3_LOW_NOISE_PARITY_ANY", any_h3),
        ("H4_MODERATE_NOISE_VIABILITY_ANY", any_h4),
        ("H5_GLOBAL_IMPROVEMENT_ANY", any_h5),
        ("H6_RANDOM_CONTROL_SANITY", h6),
        ("H7_BOUNDED_AMPLIFICATION_ANY", any_h7),
        ("H8_COST_REDUCTION_ANY", any_h8),
    ] {
        println!(
            "PGF2_3B_HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" }
        );
    }

    match selected {
        Some(arm) => {
            println!(
                "PGF2_3B_SELECTION,representation_mixing=RETAINED_FOR_COMPONENT_RESEARCH,arm={}",
                arm.name()
            );
        }
        None => {
            println!("PGF2_3B_SELECTION,representation_mixing=CLOSE,arm=NONE");
        }
    }
    println!("PGF2_3B_PGF2_4,BLOCKED");
    println!("PROMOTION,BLOCKED,pgf2_3b_component_rescue_only");
}
