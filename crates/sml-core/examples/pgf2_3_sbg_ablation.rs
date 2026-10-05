//! PGF2-3 K=2048 SBG representation ablation.
//!
//! Frozen protocol: docs/SML-PGF2-3-SBG-ABLATION-PREREGISTRATION.md.
//! This executable emits deterministic component evidence only.

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
    Pgf2,
}

impl Arm {
    const ALL: [Self; 4] = [Self::Identity, Self::Permute, Self::XorPermute, Self::Pgf2];

    const fn index(self) -> usize {
        match self {
            Self::Identity => 0,
            Self::Permute => 1,
            Self::XorPermute => 2,
            Self::Pgf2 => 3,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Identity => "IDENTITY",
            Self::Permute => "PERMUTE",
            Self::XorPermute => "XOR_PERMUTE",
            Self::Pgf2 => "PGF2",
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

fn transform(arm: Arm, input: &Field, role: &Field) -> Field {
    match arm {
        Arm::Identity => input.clone(),
        Arm::Permute => rotate_permute(input, PERMUTE_SHIFT).expect("fixed permutation"),
        Arm::XorPermute => bind(role, input).expect("qualified bind"),
        Arm::Pgf2 => {
            let mut words = input.words().to_vec();
            let stats = pascal_gf2_in_place(&mut words).expect("PGF2 transform");
            assert_eq!(stats.bits, BITS);
            assert_eq!(stats.words, WORDS);
            Field::from_words(words).expect("PGF2 field")
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
        Arm::Pgf2 => transform(Arm::Pgf2, transformed, role),
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

const fn best_control_rate(rates: &[Rate; 4]) -> u64 {
    let a = rates[Arm::Identity.index()].ppm();
    let b = rates[Arm::Permute.index()].ppm();
    let c = rates[Arm::XorPermute.index()].ppm();
    if a >= b && a >= c {
        a
    } else if b >= c {
        b
    } else {
        c
    }
}

const fn best_control_median(separations: &[Separation; 4]) -> u64 {
    let a = separations[Arm::Identity.index()].median;
    let b = separations[Arm::Permute.index()].median;
    let c = separations[Arm::XorPermute.index()].median;
    if a >= b && a >= c {
        a
    } else if b >= c {
        b
    } else {
        c
    }
}

const fn min_control_collision(values: &[u64; 4]) -> u64 {
    let a = values[Arm::Identity.index()];
    let b = values[Arm::Permute.index()];
    let c = values[Arm::XorPermute.index()];
    if a <= b && a <= c {
        a
    } else if b <= c {
        b
    } else {
        c
    }
}

fn main() {
    let role = fixed_role();
    let families = [structured_family(), balanced_random_family()];

    let mut class_separation = [[Separation::default(); 4]; 2];
    let mut route_separation = [[Separation::default(); 4]; 2];
    let mut class_collisions = [[0_u64; 4]; 2];
    let mut route_collisions = [[0_u64; 4]; 2];
    let mut low12_collisions = [[0_u64; 4]; 2];
    let mut retrieval = [[[Rate::default(); 4]; 5]; 2];
    let mut routing = [[[Rate::default(); 4]; 5]; 2];
    let mut boundary = [[[Rate::default(); 4]; 3]; 2];
    let mut inverse_checks = 0_u64;
    let mut inverse_failures = 0_u64;

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
                inverse_checks += 1;
                inverse_failures += u64::from(recover(arm, transformed, &role) != *source);
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
                "PGF2_3_SEPARATION,family={},arm={},target=CLASS,min={},median={},mean_num={},mean_den={},mean_ppm={}",
                family.name,
                arm.name(),
                class_sep.min,
                class_sep.median,
                class_sep.sum,
                class_sep.count,
                class_sep.mean_ppm(),
            );
            println!(
                "PGF2_3_SEPARATION,family={},arm={},target=ROUTE,min={},median={},mean_num={},mean_den={},mean_ppm={}",
                family.name,
                arm.name(),
                route_sep.min,
                route_sep.median,
                route_sep.sum,
                route_sep.count,
                route_sep.mean_ppm(),
            );
            println!(
                "PGF2_3_COLLISION,family={},arm={},class_full_excess={},route_full_excess={},class_low12_excess={}",
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
                    "PGF2_3_RETRIEVAL,family={},arm={},panel=UNIFORM,h={},observations={},correct={},ties={},ppm={}",
                    family.name,
                    arm.name(),
                    flips,
                    class_rate.observations,
                    class_rate.correct,
                    class_rate.ties,
                    class_rate.ppm(),
                );
                println!(
                    "PGF2_3_ROUTING,family={},arm={},panel=UNIFORM,h={},observations={},correct={},ties={},ppm={}",
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
                    "PGF2_3_RETRIEVAL,family={},arm={},panel=BOUNDARY,fraction={},observations={},correct={},ties={},ppm={}",
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

    let mut stability_by_arm = [Stability::default(); 4];
    for arm in Arm::ALL {
        let arm_index = arm.index();
        let evidence = stability(arm, &role);
        stability_by_arm[arm_index] = evidence;
        println!(
            "PGF2_3_STABILITY,arm={},observations={},min={},median={},mean_num={},mean_den={},mean_ppm_of_width={},p95={},max={}",
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
    }

    println!(
        "PGF2_3_COST,arm=IDENTITY,state_bytes=256,output_bytes=256,scratch_bytes=0,coordinate_maps=0,logical_xor_ops=0,packed_word_updates=0"
    );
    println!(
        "PGF2_3_COST,arm=PERMUTE,state_bytes=256,output_bytes=256,scratch_bytes=0,coordinate_maps=2048,logical_xor_ops=0,packed_word_updates=0"
    );
    println!(
        "PGF2_3_COST,arm=XOR_PERMUTE,state_bytes=256,output_bytes=256,scratch_bytes=0,coordinate_maps=2048,logical_xor_ops=2048,packed_word_updates=32"
    );

    let mut zero_words = vec![0_u64; WORDS];
    let pgf2_stats = pascal_gf2_in_place(&mut zero_words).expect("PGF2 cost qualification");
    println!(
        "PGF2_3_COST,arm=PGF2,state_bytes={},output_bytes={},scratch_bytes={},coordinate_maps=0,logical_xor_ops={},packed_word_updates={}",
        pgf2_stats.state_bytes,
        pgf2_stats.state_bytes,
        pgf2_stats.scratch_words * core::mem::size_of::<u64>(),
        pgf2_stats.logical_xor_ops,
        pgf2_stats.packed_word_updates,
    );

    let controls_are_isometric = (0..2).all(|family| {
        class_separation[family][0].min == class_separation[family][1].min
            && class_separation[family][0].min == class_separation[family][2].min
            && class_separation[family][0].median == class_separation[family][1].median
            && class_separation[family][0].median == class_separation[family][2].median
            && route_separation[family][0].min == route_separation[family][1].min
            && route_separation[family][0].min == route_separation[family][2].min
            && route_separation[family][0].median == route_separation[family][1].median
            && route_separation[family][0].median == route_separation[family][2].median
    });
    println!(
        "PGF2_3_CONTROL,inverse_checks={},inverse_failures={},matched_control_isometry={}",
        inverse_checks,
        inverse_failures,
        u8::from(controls_are_isometric),
    );

    let h1 = inverse_failures == 0
        && (0..2).all(|family| {
            class_collisions[family][Arm::Pgf2.index()]
                <= min_control_collision(&class_collisions[family])
                && route_collisions[family][Arm::Pgf2.index()]
                    <= min_control_collision(&route_collisions[family])
        });

    let structured_best_median = best_control_median(&class_separation[0]);
    let structured_pgf2_median = class_separation[0][Arm::Pgf2.index()].median;
    let h2 = structured_pgf2_median.saturating_mul(10) >= structured_best_median.saturating_mul(11);

    let mut retrieval_improvement = false;
    let mut retrieval_no_regression = true;
    for h_index in 0..UNIFORM_H.len() {
        let best = best_control_rate(&retrieval[0][h_index]);
        let pgf2 = retrieval[0][h_index][Arm::Pgf2.index()].ppm();
        if h_index > 0 && pgf2 >= best.saturating_add(20_000) {
            retrieval_improvement = true;
        }
        if pgf2.saturating_add(10_000) < best {
            retrieval_no_regression = false;
        }
    }
    for boundary_index in 0..BOUNDARY.len() {
        let best = best_control_rate(&boundary[0][boundary_index]);
        let pgf2 = boundary[0][boundary_index][Arm::Pgf2.index()].ppm();
        if pgf2 >= best.saturating_add(20_000) {
            retrieval_improvement = true;
        }
        if pgf2.saturating_add(10_000) < best {
            retrieval_no_regression = false;
        }
    }
    let h3 = retrieval_improvement && retrieval_no_regression;

    let mut routing_improvement = false;
    let mut routing_no_regression = true;
    for h_index in 0..UNIFORM_H.len() {
        let best = best_control_rate(&routing[0][h_index]);
        let pgf2 = routing[0][h_index][Arm::Pgf2.index()].ppm();
        if h_index > 0 && pgf2 >= best.saturating_add(20_000) {
            routing_improvement = true;
        }
        if pgf2.saturating_add(10_000) < best {
            routing_no_regression = false;
        }
    }
    let h4 = routing_improvement && routing_no_regression;

    let random_best_median_ppm = best_control_median(&class_separation[1]) * PPM / BITS as u64;
    let random_pgf2_median_ppm = class_separation[1][Arm::Pgf2.index()].median_ppm();
    let random_best_h0 = best_control_rate(&retrieval[1][0]);
    let random_pgf2_h0 = retrieval[1][0][Arm::Pgf2.index()].ppm();
    let h5 = random_pgf2_median_ppm <= random_best_median_ppm.saturating_add(20_000)
        && random_pgf2_h0 <= random_best_h0.saturating_add(20_000);

    let pgf2_stability = stability_by_arm[Arm::Pgf2.index()];
    let h6 = pgf2_stability.sum <= 128 * pgf2_stability.count && pgf2_stability.p95 <= 512;

    let h7 = pgf2_stats.bits == BITS
        && pgf2_stats.words == WORDS
        && pgf2_stats.stages == 11
        && pgf2_stats.logical_xor_ops == 11_264
        && pgf2_stats.packed_word_updates == 272
        && pgf2_stats.state_bytes == 256
        && pgf2_stats.scratch_words == 0;

    for (name, holds) in [
        ("H1_EXACTNESS", h1),
        ("H2_STRUCTURED_SEPARATION", h2),
        ("H3_RETRIEVAL_SIGNAL", h3),
        ("H4_ROUTING_SIGNAL", h4),
        ("H5_RANDOM_CONTROL_SANITY", h5),
        ("H6_LOCAL_NOISE_BOUND", h6),
        ("H7_COST_BOUND", h7),
    ] {
        println!(
            "PGF2_3_HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" }
        );
    }

    let selected = h1 && h2 && (h3 || h4) && h5 && h6 && h7;
    println!(
        "PGF2_3_SELECTION,pgf2_4={}",
        if selected { "ADVANCE" } else { "NONE" }
    );
    println!("PROMOTION,BLOCKED,pgf2_3_component_ablation_only");
}
