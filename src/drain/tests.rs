use super::shape::{Shape, has_digit, token_shape};
use super::similarity::{Comparison, Shapes, compare, digit_bits, may_pass, passes_threshold};
use super::tree::Position;
use super::*;
use crate::kayit::deneme::topla;
use crate::mask::mask;

/// Adds the messages in order; returns each cluster as "count × pattern", in the order the
/// clusters were opened.
fn cluster_one_pass(settings: Settings, messages: &[&str]) -> Vec<String> {
    let mut drain = Drain::new(settings);
    for message in messages {
        drain.add(message);
    }
    summarize(&drain.clusters)
}

/// The clusters as "count × pattern", in the order they were opened.
fn summarize(clusters: &[Cluster]) -> Vec<String> {
    clusters
        .iter()
        .map(|cluster| format!("{} × {}", cluster.count, cluster.text()))
        .collect()
}

/// A digit-free, three-letter name from an index: 0 → "aaa", 1 → "aab", 27 → "abb"
fn name(i: usize) -> String {
    [i / 676, i / 26, i]
        .iter()
        .map(|digit| char::from(b'a' + (digit % 26) as u8))
        .collect()
}

/// "Unknown user: <name>" messages for `n` different users.
fn unknown_users(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| format!("Unknown user: {}", name(i)))
        .collect()
}

#[test]
fn identical_messages_collect_in_one_cluster() {
    let result = cluster_one_pass(Settings::default(), &["Cart cache hit"; 3]);
    assert_eq!(result, ["3 × Cart cache hit"]);
}

#[test]
fn differing_position_becomes_a_wildcard() {
    let result = cluster_one_pass(
        Settings::default(),
        &[
            "Scheduled task started: rates:update",
            "Scheduled task started: backup:run",
        ],
    );
    assert_eq!(result, ["2 × Scheduled task started: <*>"]);
}

#[test]
fn messages_of_different_lengths_do_not_mix() {
    let result = cluster_one_pass(
        Settings::default(),
        &["Order <NUM> created", "Order <NUM> payment received"],
    );
    assert_eq!(
        result,
        [
            "1 × Order <NUM> created",
            "1 × Order <NUM> payment received"
        ]
    );
}

#[test]
fn similarity_threshold_decides_merging() {
    // 10 of the 16 tokens are the same: similarity 10/16 = 0.625
    let queries = [
        "Slow query (<NUM> ms): select * from `products` where `category_id` = <NUM> and `stock` > <NUM>",
        "Slow query (<NUM> ms): select * from `orders` where `user_id` = <NUM> order by `created_at` desc",
    ];
    // The default threshold is 0.7: two different queries stay apart
    assert_eq!(cluster_one_pass(Settings::default(), &queries).len(), 2);

    // Threshold 0.6: they merge, but the pattern becomes almost meaningless
    let loose = Settings {
        similarity_threshold: 0.6,
        ..Settings::default()
    };
    assert_eq!(
        cluster_one_pass(loose, &queries),
        ["2 × Slow query (<NUM> ms): select * from <*> where <*> = <NUM> <*> <*> <*> <*>"]
    );
}

#[test]
fn messages_with_different_first_tokens_are_not_compared() {
    // 2 of the 3 tokens are the same (0.67) and the threshold is low; but they are in
    // different branches, so they are never compared
    let low_threshold = Settings {
        similarity_threshold: 0.3,
        ..Settings::default()
    };
    let result = cluster_one_pass(
        low_threshold,
        &["Redis connection lost", "MySQL connection lost"],
    );
    assert_eq!(result.len(), 2);
}

#[test]
fn prefix_depth_uses_the_second_token_too() {
    // In Laravel messages the variable is often the second token
    let messages = [
        "View [emails.order] not found.",
        "View [emails.refund] not found.",
    ];
    // Depth 1: they are compared in the same leaf, 3/4 = 0.75 → they merge
    assert_eq!(
        cluster_one_pass(Settings::default(), &messages),
        ["2 × View <*> not found."]
    );

    // Depth 2: the second token chooses a branch too. Messages that differ by an ordinary
    // word fall into different leaves and never meet
    let deep = Settings {
        prefix_depth: 2,
        ..Settings::default()
    };
    let pages = ["Page cart not found", "Page refund not found"];
    assert_eq!(
        cluster_one_pass(Settings::default(), &pages),
        ["2 × Page <*> not found"]
    );
    assert_eq!(cluster_one_pass(deep.clone(), &pages).len(), 2);
    // A code-like second token (`[emails.order]`) goes to the wildcard branch, like tokens
    // with digits: template names meet at depth 2 too
    assert_eq!(
        cluster_one_pass(deep, &messages),
        ["2 × View <*> not found."]
    );
}

#[test]
fn token_with_a_digit_goes_to_the_wildcard_branch() {
    // "a1b2" and "c3d4" do not open their own branches; both meet in the wildcard branch and
    // merge
    let result = cluster_one_pass(
        Settings::default(),
        &[
            "a1b2 file deleted successfully",
            "c3d4 file deleted successfully",
        ],
    );
    assert_eq!(result, ["2 × <*> file deleted successfully"]);
}

#[test]
fn new_token_goes_to_the_wildcard_branch_in_a_full_node() {
    // At most 2 children: one for "a", the last place is kept for the wildcard branch
    let settings = Settings {
        max_children: 2,
        ..Settings::default()
    };
    let result = cluster_one_pass(settings, &["a x y z", "b x y z", "c x y z"]);
    assert_eq!(result, ["1 × a x y z", "2 × <*> x y z"]);
}

#[test]
fn empty_messages_collect_in_one_cluster() {
    let mut drain = Drain::new(Settings::default());
    drain.add("");
    drain.add("   ");
    assert_eq!(drain.clusters.len(), 1);
    assert_eq!(drain.clusters[0].count, 2);
}

#[test]
fn patterns_in_the_sample_log() {
    let (records, _) = topla(include_str!("../../ornekler/laravel.log"));
    let masked: Vec<_> = records.iter().map(|record| mask(&record.mesaj)).collect();
    let messages: Vec<&str> = masked.iter().map(|message| message.as_ref()).collect();
    let patterns = cluster_one_pass(Settings::default(), &messages);

    // Drain merges what masking could not
    assert!(
        patterns
            .iter()
            .any(|p| p == "2 × Zamanlanmış görev başladı: <*>")
    );
    // The names differ (Ayşe, Zeynep, Mehmet): the whole token "(Ayşe," becomes a wildcard
    assert!(
        patterns
            .iter()
            .any(|p| p.starts_with("3 × SQLSTATE[<NUM>]") && p.contains("values <*> <EMAIL>,"))
    );
    // The syntax is the same, so logging in and out merge too (3/4 = 0.75)
    assert!(
        patterns
            .iter()
            .any(|p| p == "9 × Kullanıcı <NUM> <*> yaptı")
    );
    // Different slow queries stay apart
    assert_eq!(
        patterns
            .iter()
            .filter(|p| p.contains("Yavaş sorgu"))
            .count(),
        3
    );

    // No position counts as variable: clustering with two passes gives the same result as
    // one pass
    let (clusters, ids) = cluster(&Settings::default(), &messages);
    assert_eq!(summarize(&clusters), patterns);
    assert_eq!(ids.len(), messages.len());
}

#[test]
fn variable_with_digits_merges_in_a_short_message() {
    // "120ms" is not masked (it is stuck to its unit); each value must not be a separate
    // pattern
    let result = cluster_one_pass(
        Settings::default(),
        &[
            "Response time 120ms",
            "Response time 95ms",
            "Response time 300ms",
        ],
    );
    assert_eq!(result, ["3 × Response time <*>"]);
}

#[test]
fn difference_without_digits_does_not_merge_in_a_short_message() {
    // "received" and "rejected" are different events; there is no digit, the threshold must not
    // relax
    let result = cluster_one_pass(
        Settings::default(),
        &["Payment received", "Payment rejected"],
    );
    assert_eq!(result.len(), 2);
}

#[test]
fn token_shapes() {
    use Shape::*;
    let examples = [
        (r"App\Jobs\SendInvoice", Class),
        (r"\App\Models\Product", Class),
        (r"App\\Jobs\\SendInvoice", Class),
        (r"[App\Jobs\SendInvoice]", Class),
        (r"App\Jobs\SendInvoice:", Class),
        (r"resources\views\product.blade.php", Other),
        (r"App\Jobs\X::handle()", Other),
        (r"\", Other),
        ("ORDER_CREATED", Constant),
        ("ORDER_CREATED,", Constant),
        ("HTTP_2", Constant),
        ("SendInvoice", Identifier),
        ("rateLimit", Identifier),
        ("send_invoice", Identifier),
        ("Order_Created", Identifier),
        ("$product", Identifier),
        ("$product2", Identifier),
        ("payment.failed", Dotted),
        ("rates:update", Dotted),
        ("example.test", Dotted),
        ("mail.from.address", Dotted),
        ("(order.create)", Dotted),
        ("'price'", Quoted),
        ("\"dsn\"", Quoted),
        ("'App\\Jobs\\X'", Quoted),
        ("`products`", Other),
        ("''", Other),
        ("\"a'b\"", Other),
        ("\"'\"", Other),
        ("orderV2Create", Identifier),
        ("ApiV2Client", Identifier),
        ("Invoice2Send", Other),
        (r"\App", Other),
        (r"App\", Other),
        (r"App\\", Other),
        ("'Ali", Word),
        ("Veli'", Word),
        ("Café", Word),
        ("received", Word),
        ("GET", Word),
        ("SQLSTATE", Word),
        ("Handled:", Word),
        ("<NUM>", Other),
        ("<*>", Other),
        (",", Other),
        ("(,)", Other),
        ("oauth2", Other),
        ("a1b2", Other),
        ("=", Other),
        ("", Other),
        ("$", Other),
        ("e-mail", Other),
        ("..", Other),
        (".a", Other),
    ];
    for (token, expected) in examples {
        assert_eq!(token_shape(token), expected, "{token:?}");
    }
    assert!(Class.is_code() && Constant.is_code() && Identifier.is_code());
    assert!(Dotted.is_code() && Quoted.is_code());
    assert!(!Word.is_code() && !Other.is_code());
}

#[test]
fn code_like_values_of_the_same_shape_merge() {
    // A categorical position: the two messages differ only in two code-like values of the
    // same shape. All have 2–3 tokens: with a difference without digits they would stay apart
    // (0.5 or 0.67 < 0.7)
    let pairs: [(&str, &str, &str); 7] = [
        (
            r"Handled: App\Jobs\SendInvoice",
            r"Handled: App\Jobs\SendEmail",
            "Handled: <*>",
        ),
        (
            "Status: ORDER_CREATED",
            "Status: ORDER_FAILED",
            "Status: <*>",
        ),
        (
            "Event: payment.failed",
            "Event: payment.succeeded",
            "Event: <*>",
        ),
        (
            "Undefined variable $product",
            "Undefined variable $order",
            "Undefined variable <*>",
        ),
        (
            "Undefined array key \"price\"",
            "Undefined array key \"stock\"",
            "Undefined array key <*>",
        ),
        ("Queue: send_invoice", "Queue: send_email", "Queue: <*>"),
        (
            "Listener: OrderCreated",
            "Listener: PaymentReceived",
            "Listener: <*>",
        ),
    ];
    for (first, second, pattern) in pairs {
        assert_eq!(
            cluster_one_pass(Settings::default(), &[first, second]),
            [format!("2 × {pattern}")],
            "{first:?} / {second:?}"
        );
    }
    // In classic Drain they do not merge
    let classic = Settings {
        flexible_threshold: false,
        ..Settings::default()
    };
    assert_eq!(
        cluster_one_pass(classic, &["Status: ORDER_CREATED", "Status: ORDER_FAILED"]).len(),
        2
    );
}

#[test]
fn ordinary_words_and_different_shapes_do_not_merge() {
    let pairs = [
        // Ordinary words are different events
        ("Queue emails full", "Queue default full"),
        ("GET /api/products slow", "POST /api/products slow"),
        // A code-like value and an ordinary word
        ("Status: ORDER_FAILED", "Status: ready"),
        // Different code-like shapes
        ("Status: ORDER_FAILED", "Status: payment.pending"),
        // Backtick-quoted SQL names: different tables are different queries
        ("Table `products` locked", "Table `orders` locked"),
        // Of the tokens that mix letters and digits, only one has a digit
        ("oauth2 token refreshed", "oauth token refreshed"),
    ];
    for (first, second) in pairs {
        assert_eq!(
            cluster_one_pass(Settings::default(), &[first, second]).len(),
            2,
            "{first:?} / {second:?}"
        );
    }
}

#[test]
fn code_like_head_token_goes_to_the_wildcard_branch() {
    // The head token chooses the branch; if a code-like head token opened a separate branch,
    // the two events would never be compared
    let result = cluster_one_pass(
        Settings::default(),
        &[
            "ORDER_CREATED event handled",
            "ORDER_FAILED event handled",
            r"App\Jobs\SendInvoice handled",
            r"App\Jobs\SendEmail handled",
        ],
    );
    assert_eq!(result, ["2 × <*> event handled", "2 × <*> handled"]);
    // Ordinary words open their own branches; they are not in the same leaf
    let result = cluster_one_pass(
        Settings::default(),
        &["Payment event handled", "Shipping event handled"],
    );
    assert_eq!(result.len(), 2);
}

#[test]
fn code_like_value_facing_a_wildcard_counts_as_variable() {
    // Once the pattern is "Status: <*>", a code-like value joins it and an ordinary word
    // opens a separate cluster
    let result = cluster_one_pass(
        Settings::default(),
        &[
            "Status: ORDER_CREATED",
            "Status: ORDER_FAILED",
            "Status: payment.pending",
            "Status: ready",
        ],
    );
    assert_eq!(result, ["3 × Status: <*>", "1 × Status: ready"]);
}

#[test]
fn code_like_difference_alone_is_no_foothold() {
    // A merge that keeps none of the tokens only happens with differences with digits
    let result = cluster_one_pass(Settings::default(), &["A_B C_D", "E_F G_H"]);
    assert_eq!(result.len(), 2);
    // One-word code-like messages do not merge either: every event name is a separate event
    let result = cluster_one_pass(
        Settings::default(),
        &["payment.failed", "payment.succeeded", "ORDER_FAILED"],
    );
    assert_eq!(result.len(), 3);
}

#[test]
fn token_core_strips_the_sentence() {
    assert_eq!(token_core("(Ayşe,"), "Ayşe");
    assert_eq!(token_core(r"[App\Jobs\X]"), r"App\Jobs\X");
    assert_eq!(token_core("\"price\":"), "\"price\"");
    assert_eq!(token_core("login"), "login");
    assert_eq!(token_core(","), ",");
    assert_eq!(token_core("([{}])"), "([{}])");
    assert_eq!(token_core(""), "");
}

#[test]
fn wildcard_positions_are_read_from_the_pattern() {
    let mut drain = Drain::new(Settings::default());
    drain.add("Template <*> missing: a1 x");
    drain.add("Template <*> missing: b2 y");
    let cluster = &drain.clusters[0];
    assert_eq!(cluster.text(), "Template <*> missing: <*> <*>");
    // The "<*>" written in the message (position 1) is not a wildcard
    assert_eq!(cluster.wildcard_positions().collect::<Vec<_>>(), [3, 4]);
}

#[test]
fn fixed_word_with_a_digit_does_not_relax_the_threshold() {
    // "S3" is the same in both messages; the word that differs has no digit, the threshold
    // must not relax
    let result = cluster_one_pass(
        Settings::default(),
        &["S3 upload completed", "S3 upload failed"],
    );
    assert_eq!(result.len(), 2);
}

#[test]
fn position_with_more_than_a_hundred_values_is_variable() {
    // 100 different names: the limit is not exceeded, every name is a separate pattern
    let messages = unknown_users(101);
    let (clusters, _) = cluster(&Settings::default(), &messages[..100]);
    assert_eq!(clusters.len(), 100);

    // The 101st name makes the position variable. The second pass knows this: the first 100
    // names merge too. Another leaf (Payment) is not affected.
    let mut all = messages.clone();
    all.extend([
        "Payment received".to_string(),
        "Payment rejected".to_string(),
    ]);
    let (clusters, ids) = cluster(&Settings::default(), &all);
    assert_eq!(
        summarize(&clusters),
        [
            "101 × Unknown user: <*>",
            "1 × Payment received",
            "1 × Payment rejected"
        ]
    );
    assert!(ids[..101].iter().all(|&id| id == 0));
    assert_eq!(ids[101..], [1, 2]);
}

#[test]
fn later_passes_do_not_forget_what_earlier_ones_learned() {
    // Live watching: in the first pass a variable position is found in a leaf and a second
    // pass is set up; then the messages that come later find a variable position in another
    // leaf and a third pass is set up. The third pass must know both.
    let mut first = Clusterer::new(Settings::default());
    for (id, message) in unknown_users(101).iter().enumerate() {
        first.add_text(id, message);
    }
    assert!(first.needs_second_pass());
    let mut second = first.second_pass();
    assert_eq!(second.drain.learned.len(), 1);
    assert!(!second.needs_second_pass());
    let jobs: Vec<String> = (0..101)
        .map(|i| format!("Job finished: {}", name(i)))
        .collect();
    for (id, message) in jobs.iter().enumerate() {
        second.add_text(200 + id, message);
    }
    assert!(second.needs_second_pass());
    let third = second.second_pass();
    assert_eq!(third.drain.learned.len(), 2);
    // The result: both families in a single pattern from the start
    let mut result = third;
    for (id, message) in unknown_users(101).iter().enumerate() {
        result.add_text(id, message);
    }
    for (id, message) in jobs.iter().enumerate() {
        result.add_text(200 + id, message);
    }
    assert_eq!(
        summarize(result.clusters()),
        ["101 × Unknown user: <*>", "101 × Job finished: <*>"]
    );
    // The copy that carries only the clusters: the same patterns, an empty tree
    let copy = result.clusters_only();
    assert_eq!(summarize(copy.clusters()), summarize(result.clusters()));
    assert!(copy.drain.by_length.is_empty());
}

#[test]
fn the_limit_is_more_than_a_hundred_different_values() {
    // 101 clusters, but exactly 100 different values in the last position: "aaa" appears in two
    // clusters (the second differs by the middle word). It does not exceed 100, so the
    // position does not count as variable.
    let mut messages = unknown_users(100);
    messages.push("Unknown person: aaa".to_string());
    let (clusters, _) = cluster(&Settings::default(), &messages);
    assert_eq!(clusters.len(), 101);
    // One more name: 101 different values, the position is variable
    messages.push(format!("Unknown user: {}", name(100)));
    let (clusters, _) = cluster(&Settings::default(), &messages);
    assert_eq!(
        summarize(&clusters),
        ["101 × Unknown user: <*>", "1 × Unknown person: aaa"]
    );
}

#[test]
fn variable_position_stops_the_leaf_from_growing() {
    // In one pass: the first 101 names open separate clusters, the following ones merge into
    // the first cluster thanks to the variable position. The leaf does not grow; every new
    // message is compared with at most 101 clusters. Without the rule 5 000 clusters would be
    // opened, and the last message would be compared with 4 999 clusters.
    let mut drain = Drain::new(Settings::default());
    for message in unknown_users(5_000) {
        drain.add(&message);
    }
    let clusters = &drain.clusters;
    assert_eq!(clusters.len(), 101);
    assert_eq!(clusters[0].text(), "Unknown user: <*>");
    assert_eq!(clusters[0].count, 4_900);
    assert!(clusters[1..].iter().all(|cluster| cluster.count == 1));
}

#[test]
fn variable_position_counts_toward_the_threshold_like_a_digit_difference() {
    // The limit is 2 different values: the third name makes the position variable. In the
    // second pass, the difference of "carol", who joins the cluster of "alice", counts as
    // variable: threshold 0.7 × 2/3 = 0.47 ≤ 0.67
    let settings = Settings {
        max_children: 2,
        ..Settings::default()
    };
    let messages = [
        "Unknown user: alice",
        "Unknown user: bob",
        "Unknown user: carol",
    ];
    let (clusters, ids) = cluster(&settings, &messages);
    assert_eq!(summarize(&clusters), ["3 × Unknown user: <*>"]);
    assert_eq!(ids, [0, 0, 0]);
    // In one pass, the first two names open separate clusters before the position counts as
    // variable
    assert_eq!(cluster_one_pass(settings, &messages).len(), 3);
}

#[test]
fn flexible_threshold_can_be_turned_off() {
    // When it is off, classic Drain: 2/3 = 0.67 < 0.7, so every value stays apart
    let classic = Settings {
        flexible_threshold: false,
        ..Settings::default()
    };
    let result = cluster_one_pass(
        classic.clone(),
        &["Response time 120ms", "Response time 95ms"],
    );
    assert_eq!(result.len(), 2);
    // No variable position either: 101 names, 101 patterns
    let (clusters, _) = cluster(&classic, &unknown_users(101));
    assert_eq!(clusters.len(), 101);
}

#[test]
fn names_are_digit_free_and_distinct() {
    let names: HashSet<String> = (0..17_576).map(name).collect();
    assert_eq!(names.len(), 17_576);
    assert!(names.iter().all(|name| !has_digit(name)));
    assert_eq!(
        (name(0), name(1), name(27)),
        ("aaa".into(), "aab".into(), "abb".into())
    );
}

/// `n` different, digit-free, one-word event names: "event.aaa", "event.aab"...
fn events(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("event.{}", name(i))).collect()
}

#[test]
fn positions_are_not_variable_in_one_word_messages() {
    // All one-word messages are in the same leaf. More than a hundred different event names
    // must not merge them; if they did, two separate events among them would be lost too.
    let mut messages = events(101);
    messages.extend(["DatabaseDown".to_string(), "DiskFull".to_string()]);
    let (clusters, ids) = cluster(&Settings::default(), &messages);
    assert_eq!(clusters.len(), 103);
    assert_eq!(ids, (0..103).collect::<Vec<_>>());
    // The positions are never counted: no second pass is needed either
    let mut drain = Drain::new(Settings::default());
    for message in &messages {
        drain.add(message);
    }
    assert!(drain.variables.is_empty());
}

#[test]
fn on_a_tie_the_one_with_more_wildcards_wins() {
    // The last message shares two tokens with both clusters (threshold 0.5). Like Drain3, it
    // chooses the more general pattern, that is, the one with more wildcards; so the cluster
    // opened first does not change. The result is the same as Drain3 0.9.11 (depth=4,
    // sim_th=0.5).
    let settings = Settings {
        similarity_threshold: 0.5,
        flexible_threshold: false,
        ..Settings::default()
    };
    let messages = ["a b c e", "a x y d", "a z w d", "a b q d"];
    assert_eq!(
        cluster_one_pass(settings.clone(), &messages),
        ["1 × a b c e", "3 × a <*> <*> d"]
    );
    // With the flexible threshold too (no digits in the differences)
    let flexible = Settings {
        flexible_threshold: true,
        ..settings
    };
    assert_eq!(
        cluster_one_pass(flexible, &messages),
        ["1 × a b c e", "3 × a <*> <*> d"]
    );
}

#[test]
fn the_most_similar_of_the_ones_passing_the_threshold_wins() {
    // The third message shares two tokens with the first cluster, but its differences are not
    // variable: threshold 0.7, similarity 0.5. It shares a single token with the second
    // cluster, but the other three differences have a digit on both sides: threshold
    // 0.7 × 1/4 = 0.175, similarity 0.25. Even if the most similar cluster does not pass the
    // threshold, the second one does.
    let result = cluster_one_pass(
        Settings::default(),
        &["Payment x y c3", "Payment a9 b9 c9", "Payment a1 b2 c3"],
    );
    assert_eq!(result, ["1 × Payment x y c3", "2 × Payment <*> <*> <*>"]);
}

#[test]
fn a_digit_on_one_side_only_is_not_a_variable_difference() {
    let result = cluster_one_pass(Settings::default(), &["Unauthenticated.", "user42"]);
    assert_eq!(result, ["1 × Unauthenticated.", "1 × user42"]);
    let result = cluster_one_pass(
        Settings::default(),
        &["Response time 120ms", "Response time exceeded"],
    );
    assert_eq!(result.len(), 2);
}

#[test]
fn one_word_messages_with_digits_merge() {
    // A difference with a digit on both sides is variable: one-word messages with digits (IDs,
    // hashes) collect in one cluster; the ones without digits merge only if identical
    let result = cluster_one_pass(
        Settings::default(),
        &[
            "Unauthenticated.",
            "user42",
            "user43",
            "Forbidden",
            "a1b2c3",
            "Unauthenticated.",
        ],
    );
    assert_eq!(result, ["2 × Unauthenticated.", "3 × <*>", "1 × Forbidden"]);
    // A merge that keeps none of the tokens happens if every difference has digits
    let result = cluster_one_pass(Settings::default(), &["a1b2 c3d4", "e5f6 g7h8"]);
    assert_eq!(result, ["2 × <*> <*>"]);
    // Threshold 0: as in classic Drain, everything in the same leaf merges
    let zero = Settings {
        similarity_threshold: 0.0,
        ..Settings::default()
    };
    let result = cluster_one_pass(zero, &["Unauthenticated.", "Forbidden", "user42"]);
    assert_eq!(result, ["3 × <*>"]);
}

#[test]
fn one_shared_word_is_not_enough_in_the_wildcard_leaf() {
    // "Code7" has a digit, so these messages are in the wildcard branch; 101 names make the
    // last position variable. "Report file ready", which falls into the same leaf, shares only
    // "file": the difference between "Report" and "Code7" has a digit on one side only.
    let mut messages: Vec<String> = (0..101)
        .map(|i| format!("Code7 file {}", name(i)))
        .collect();
    messages.push("Report file ready".to_string());
    let (clusters, _) = cluster(&Settings::default(), &messages);
    assert_eq!(
        summarize(&clusters),
        ["101 × Code7 file <*>", "1 × Report file ready"]
    );
}

#[test]
fn variable_position_alone_is_no_foothold() {
    // Two-token messages in the wildcard branch: 101 names make the second position variable.
    // "S3 error" and "Code7 <*>" have no token in common: the first position has a digit on
    // both sides, the second is only a variable position. If they merged, the error record
    // would be lost among a hundred and one other messages.
    let mut messages: Vec<String> = (0..101).map(|i| format!("Code7 {}", name(i))).collect();
    messages.push("S3 error".to_string());
    let (clusters, _) = cluster(&Settings::default(), &messages);
    assert_eq!(summarize(&clusters), ["101 × Code7 <*>", "1 × S3 error"]);
}

#[test]
fn wildcard_written_in_a_message_is_an_ordinary_word() {
    // The wildcard in patterns is not mistaken for a word in a message: identical messages that
    // contain a written "<*>" merge. If it were mistaken, each would open a separate cluster,
    // and the time would grow with the square of the number of messages.
    let messages = vec!["Template <*> missing"; 5_000];
    let (clusters, _) = cluster(&Settings::default(), &messages);
    assert_eq!(summarize(&clusters), ["5000 × Template <*> missing"]);
    // A "<*>" at the start does not go to the wildcard branch, it opens its own branch
    let result = cluster_one_pass(Settings::default(), &["<*> ready", "<*> ready", "x ready"]);
    assert_eq!(result, ["2 × <*> ready", "1 × x ready"]);
    // In classic Drain too
    let classic = Settings {
        flexible_threshold: false,
        ..Settings::default()
    };
    assert_eq!(cluster(&classic, &["Template <*> missing"; 3]).0.len(), 1);
}

/// Repeatable pseudo-random numbers (xorshift64*).
struct Rng(u64);

impl Rng {
    /// A number between 0 and `bound` (`bound` excluded).
    fn below(&mut self, bound: usize) -> usize {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) % bound as u64) as usize
    }
}

/// Clusters the messages with `Clusterer`; two passes if needed, like `cluster`. Identical
/// texts get the same ID.
fn cluster_cached(settings: &Settings, messages: &[String]) -> (Vec<String>, Vec<usize>) {
    let mut ids_by_text: HashMap<&str, usize> = HashMap::new();
    let ids: Vec<usize> = messages
        .iter()
        .map(|message| {
            let next_id = ids_by_text.len();
            *ids_by_text.entry(message.as_str()).or_insert(next_id)
        })
        .collect();
    let pass = |clusterer: &mut Clusterer| -> Vec<usize> {
        messages
            .iter()
            .zip(&ids)
            .map(|(message, &id)| clusterer.add_text(id, message))
            .collect()
    };
    let mut clusterer = Clusterer::new(settings.clone());
    let mut result = pass(&mut clusterer);
    if clusterer.needs_second_pass() {
        clusterer = clusterer.second_pass();
        result = pass(&mut clusterer);
    }
    (summarize(clusterer.clusters()), result)
}

#[test]
fn same_message_later_joins_a_more_similar_cluster() {
    // Drain depends on order: the fourth message is the same as the second, but the cluster
    // opened in between resembles it more (6 of its 7 tokens, 5 in the first cluster). Every
    // record must be clustered in its own turn; if the same message were clustered only once,
    // the fourth would fall into the first cluster too.
    let messages: Vec<String> = [
        "Cart sync failed session closed mobile app",
        "Cart sync failed session closed web browser",
        "Cart clear failed session closed web browser",
        "Cart sync failed session closed web browser",
    ]
    .map(String::from)
    .into();
    let (clusters, ids) = cluster_cached(&Settings::default(), &messages);
    assert_eq!(ids, [0, 0, 1, 1]);
    assert_eq!(
        clusters,
        [
            "2 × Cart sync failed session closed <*> <*>",
            "2 × Cart <*> failed session closed web browser",
        ]
    );
    let (reference, reference_ids) = cluster(&Settings::default(), &messages);
    assert_eq!((summarize(&reference), reference_ids), (clusters, ids));
}

#[test]
fn cached_clustering_matches_uncached() {
    // The cache only makes it faster: with every setting and every sequence of messages, the
    // result must be exactly the same as adding every message with a search (including the
    // two passes)
    let settings_list = [
        Settings::default(),
        Settings {
            prefix_depth: 2,
            max_children: 3,
            ..Settings::default()
        },
        Settings {
            prefix_depth: 3,
            max_children: 4,
            similarity_threshold: 0.4,
            ..Settings::default()
        },
        Settings {
            flexible_threshold: false,
            similarity_threshold: 0.5,
            max_children: 5,
            ..Settings::default()
        },
        Settings {
            similarity_threshold: 1.0,
            ..Settings::default()
        },
        Settings {
            similarity_threshold: 0.0,
            ..Settings::default()
        },
    ];
    let words = [
        "cart", "payment", "shipping", "web", "mobile", "api", "open", "closed",
    ];
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut two_pass = 0;
    for trial in 0..60 {
        let settings = &settings_list[trial % settings_list.len()];
        // A small pool of messages in every trial: repeats test the cache, the random order
        // tests that Drain depends on order, many different names test the second pass
        let pool_size = 2 + rng.below(40);
        let pool: Vec<String> = (0..pool_size)
            .map(|_| match rng.below(6) {
                0 => format!("Unknown user: {}", name(rng.below(400))),
                1 => name(rng.below(20)),
                2 => format!("Response time {}ms", rng.below(500)),
                3 => String::new(),
                _ => {
                    let len = 2 + rng.below(5);
                    let tokens: Vec<&str> =
                        (0..len).map(|_| words[rng.below(words.len())]).collect();
                    tokens.join(" ")
                }
            })
            .collect();
        let count = 50 + rng.below(400);
        let mut messages: Vec<String> = (0..count)
            .map(|_| pool[rng.below(pool.len())].clone())
            .collect();
        // In some trials hundreds of different names: a variable position in a leaf, a
        // second pass
        if trial % 4 == 0 {
            for i in 0..250 {
                let at = rng.below(messages.len());
                messages.insert(at, format!("Unknown user: {}", name(i)));
            }
        }
        let (reference, reference_ids) = cluster(settings, &messages);
        let (clusters, ids) = cluster_cached(settings, &messages);
        assert_eq!(ids, reference_ids, "trial {trial}");
        assert_eq!(clusters, summarize(&reference), "trial {trial}");
        let mut drain = Clusterer::new(settings.clone());
        for (id, message) in messages.iter().enumerate() {
            drain.add_text(id, message);
        }
        two_pass += usize::from(drain.needs_second_pass());
    }
    // The second pass was really tested
    assert!(two_pass >= 5, "{two_pass}");
}

#[test]
fn token_bounds_match_split_whitespace() {
    for message in [
        "",
        "   ",
        "one",
        "  two  words ",
        "tab\tand\nline end\r\n",
        "Unicode\u{00a0}space\u{2003}and\u{3000}ideographic",
        "multi-byte ğüşıöç İ 🚀",
        "\u{feff}BOM is not whitespace",
    ] {
        let expected: Vec<&str> = message.split_whitespace().collect();
        let actual: Vec<&str> = token_bounds(message)
            .iter()
            .map(|&(start, end)| &message[start..end])
            .collect();
        assert_eq!(actual, expected, "{message:?}");
    }
}

#[test]
fn pattern_shares_the_text_of_the_opening_message() {
    let message: Rc<str> = Rc::from("User <NUM> logged in");
    let mut clusterer = Clusterer::new(Settings::default());
    clusterer.add(0, &message);
    clusterer.add_text(1, "User <NUM> logged out");
    let cluster = &clusterer.clusters()[0];
    assert!(Rc::ptr_eq(&cluster.origin, &message));
    assert_eq!(cluster.text(), "User <NUM> logged <*>");
    assert_eq!(cluster.wildcard_positions().collect::<Vec<_>>(), [3]);
    // A `<*>` written in the message is not a wildcard
    clusterer.add_text(2, "Value: <*>");
    assert_eq!(clusterer.clusters()[1].wildcard_positions().count(), 0);
}

#[test]
fn in_a_full_leaf_dissimilar_messages_go_to_the_overflow_cluster() {
    // Messages whose every position takes few values but whose combinations are countless: none
    // resembles another (at most 1 of 3 tokens is the same). Once the leaf is full, the new ones
    // collect in the overflow cluster; a message is compared with at most the clusters in the
    // leaf.
    for flexible_threshold in [false, true] {
        let settings = Settings {
            max_children: 3,
            flexible_threshold,
            ..Settings::default()
        };
        let limit = LEAF_FACTOR * settings.max_children;
        let messages: Vec<String> = (0..limit + 20)
            .map(|i| format!("event {} {}", name(i), name(i + 7_000)))
            .collect();
        let mut drain = Drain::new(settings.clone());
        for message in &messages {
            drain.add(message);
        }
        if flexible_threshold {
            // With the flexible threshold, the positions are variable once they take more
            // than `max_children` values: in the second pass everything is a single pattern
            assert!(!drain.variables.is_empty());
            let (clusters, _) = cluster(&settings, &messages);
            assert_eq!(
                summarize(&clusters),
                [format!("{} × event <*> <*>", limit + 20)]
            );
            continue;
        }
        assert_eq!(drain.clusters.len(), limit + 1);
        let overflow = &drain.clusters[limit];
        assert_eq!(
            (overflow.count, overflow.text().as_str()),
            (20, "event <*> <*>")
        );
        let leaf = drain.leaf_mut(&["event", "x", "y"]).unwrap();
        assert_eq!(leaf.cluster_ids.len(), limit);
        assert_eq!(leaf.overflow, Some(limit));
        // A message that resembles a cluster in the leaf still joins it
        assert_eq!(drain.add(&messages[3]), 3);
        // Clustering with the cache gives the same result
        let mut clusterer = Clusterer::new(settings.clone());
        let ids: Vec<usize> = messages
            .iter()
            .chain(&messages)
            .enumerate()
            .map(|(id, message)| clusterer.add_text(id % messages.len(), message))
            .collect();
        let twice: Vec<&String> = messages.iter().chain(&messages).collect();
        let (reference, reference_ids) = cluster(&settings, &twice);
        assert_eq!(ids, reference_ids);
        assert_eq!(summarize(clusterer.clusters()), summarize(&reference));
    }
    // The leaf of single-token messages does not fill up (every message is compared with at
    // most two clusters anyway): every different word is its own cluster, past the limit of
    // the leaf too
    for flexible_threshold in [false, true] {
        let settings = Settings {
            max_children: 3,
            flexible_threshold,
            ..Settings::default()
        };
        let words: Vec<String> = (0..LEAF_FACTOR * 3 + 20).map(name).collect();
        let (clusters, _) = cluster(&settings, &words);
        assert_eq!(clusters.len(), words.len());
    }
}

#[test]
fn cache_searches_again_after_the_tree_changes() {
    // "a b c d" first opens its own cluster; then a message of the same length changes that
    // cluster's pattern. When "a b c d" comes again, the result in the cache is stale; it is
    // searched again.
    let mut clusterer = Clusterer::new(Settings::default());
    assert_eq!(clusterer.add_text(0, "a b c d"), 0);
    assert_eq!(clusterer.add_text(1, "a b c e"), 0);
    assert_eq!(clusterer.add_text(0, "a b c d"), 0);
    // A change in another length does not spoil the cache
    assert_eq!(clusterer.add_text(2, "x y"), 1);
    assert_eq!(clusterer.add_text(0, "a b c d"), 0);
    assert_eq!(
        summarize(clusterer.clusters()),
        ["4 × a b c <*>", "1 × x y"]
    );
}

#[test]
fn single_token_shortcut_matches_a_full_scan() {
    // A single-token message is compared with at most two candidates instead of all the
    // clusters in the leaf. With every setting the result must be the same as scanning all
    // the clusters.
    for (flexible_threshold, similarity_threshold) in [
        (true, 0.7),
        (true, 1.0),
        (true, 0.3),
        (true, 0.0),
        (false, 0.7),
        (false, 0.0),
    ] {
        let settings = Settings {
            flexible_threshold,
            similarity_threshold,
            ..Settings::default()
        };
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let mut drain = Drain::new(settings);
        for _ in 0..3_000 {
            // Digit-free names, IDs with digits and a few fixed words
            let message = match rng.below(4) {
                0 => name(rng.below(300)),
                1 => format!("k{}", rng.below(60)),
                2 => ["Unauthenticated.", "<*>", "a1b2", "ok"][rng.below(4)].to_string(),
                _ => format!("{}{}", name(rng.below(5)), rng.below(3)),
            };
            let tokens = [message.as_str()];
            let full_scan = drain
                .by_length
                .get(&1)
                .and_then(|leaf| drain.most_similar(&leaf.cluster_ids, &tokens, &leaf.positions));
            assert_eq!(
                drain.search(&tokens),
                full_scan.map_or(Search::NotFound, Search::Found),
                "{message}"
            );
            drain.add(&message);
        }
    }
}

// ---- Early rejection in the candidate comparison

/// The comparison without early rejection, as it was before: the reference the early-rejecting
/// `compare` is checked against.
fn compare_to_the_end(
    cluster: &Cluster,
    tokens: &[&str],
    positions: &[Position],
) -> (usize, usize, usize, usize) {
    let (mut equal, mut wildcards, mut variable_diffs, mut digit_diffs) = (0, 0, 0, 0);
    for (i, (p, t)) in cluster.tokens().zip(tokens).enumerate() {
        let (digits, code) = if p == WILDCARD {
            wildcards += 1;
            (has_digit(t), token_shape(t).is_code())
        } else if p == *t {
            equal += 1;
            continue;
        } else {
            let shape_t = token_shape(t);
            (
                has_digit(p) && has_digit(t),
                shape_t.is_code() && token_shape(p) == shape_t,
            )
        };
        if digits {
            digit_diffs += 1;
        }
        if digits || code || positions.get(i).is_some_and(Position::is_variable) {
            variable_diffs += 1;
        }
    }
    (equal, wildcards, variable_diffs, digit_diffs)
}

/// A cluster opened by `pattern` whose positions `wildcards` are wildcards.
fn pattern_cluster(pattern: &str, wildcards: &[usize]) -> Cluster {
    let origin: Rc<str> = Rc::from(pattern);
    let mut cluster = Cluster::new(&origin, &token_bounds(pattern));
    let mut other: Vec<&str> = pattern.split_whitespace().collect();
    for &i in wildcards {
        other[i] = "\u{1}";
    }
    cluster.update(&other);
    cluster
}

fn settings_with(threshold: f64, flexible: bool) -> Settings {
    Settings {
        similarity_threshold: threshold,
        flexible_threshold: flexible,
        ..Settings::default()
    }
}

/// Every (pattern, message) pair of the fixture, with the leaf positions for the flexible
/// threshold: variable at index 6, so that `positions` plays a part too.
fn comparison_fixture() -> (Vec<Cluster>, Vec<Vec<&'static str>>, Vec<Position>) {
    let patterns = [
        (
            "Job App\\Jobs\\SendInvoice ORDER_CREATED took 12ms for user_42 on node-7",
            vec![],
        ),
        (
            "Job App\\Jobs\\SendInvoice ORDER_CREATED took 12ms for user_42 on node-7",
            vec![4, 6],
        ),
        (
            "Payment received from Ayşe for order 100 at 10:00:00",
            vec![],
        ),
        (
            "Payment received from Ayşe for order 100 at 10:00:00",
            vec![3],
        ),
        ("a b c d e f g h i j", vec![]),
        (
            "<NUM> <NUM> <NUM> <NUM> <NUM> <NUM> <NUM> <NUM> <NUM> <NUM>",
            vec![],
        ),
        ("one two", vec![]),
        ("one two", vec![0]),
    ];
    let messages = [
        "Job App\\Jobs\\SendInvoice ORDER_CREATED took 12ms for user_42 on node-7",
        "Job App\\Jobs\\SendSms ORDER_FAILED took 40ms for user_9 on node-8",
        "Job failed badly then took 12ms for user_42 on node-7",
        "Job App\\Jobs\\SendInvoice ORDER_CREATED took 12ms for nobody on earth",
        "Payment received from Ali for order 101 at 10:00:01",
        "Payment rejected from Ali for order 100 at 10:00:00",
        "Refund received from Ayşe for order 100 at 10:00:00",
        "x y z d e f g h i j",
        "a b c d e f g h i k",
        "1 2 3 4 5 6 7 8 9 10",
        "k l m n o p q r s t",
        "one two",
        "one three",
        "two two",
    ];
    let mut positions: Vec<Position> = (0..10).map(|_| Position::Values(HashSet::new())).collect();
    positions[6] = Position::Variable;
    (
        patterns
            .iter()
            .map(|(pattern, wildcards)| pattern_cluster(pattern, wildcards))
            .collect(),
        messages
            .iter()
            .map(|m| m.split_whitespace().collect())
            .collect(),
        positions,
    )
}

#[test]
fn early_rejection_never_drops_a_cluster_that_passes() {
    let (clusters, messages, positions) = comparison_fixture();
    let thresholds = [0.0, 0.1, 0.3, 0.5, 0.6, 0.7, 0.75, 0.8, 0.9, 0.99, 1.0];
    let mut checked = 0;
    let mut rejected_early = 0;
    for flexible in [true, false] {
        for &t in &thresholds {
            let settings = settings_with(t, flexible);
            for cluster in &clusters {
                for tokens in &messages {
                    if cluster.tokens().count() != tokens.len() {
                        continue;
                    }
                    let (equal, wildcards, variable_diffs, digit_diffs) =
                        compare_to_the_end(cluster, tokens, &positions);
                    let reference = Comparison {
                        equal,
                        wildcards,
                        variable_diffs,
                        digit_diffs,
                    };
                    let passes = passes_threshold(&settings, &reference, tokens.len());
                    let mut shapes = Shapes::new(tokens);
                    let result = compare(
                        &settings,
                        cluster,
                        tokens,
                        &positions,
                        &mut shapes,
                        digit_bits(tokens),
                        None,
                    );
                    match result {
                        Some(r) => {
                            assert_eq!(
                                (r.equal, r.wildcards, r.variable_diffs, r.digit_diffs),
                                (equal, wildcards, variable_diffs, digit_diffs),
                                "t={t} flexible={flexible} {tokens:?}"
                            );
                        }
                        None => {
                            assert!(
                                !passes,
                                "rejected early but passes: t={t} flexible={flexible} {tokens:?}"
                            );
                            rejected_early += 1;
                        }
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 300, "{checked}");
    assert!(
        rejected_early > 50,
        "early rejection was really exercised: {rejected_early}"
    );
}

#[test]
fn early_rejection_by_the_best_so_far_keeps_every_possible_winner() {
    let (clusters, messages, positions) = comparison_fixture();
    let settings = Settings::default();
    for cluster in &clusters {
        for tokens in &messages {
            if cluster.tokens().count() != tokens.len() {
                continue;
            }
            let (equal, ..) = compare_to_the_end(cluster, tokens, &positions);
            for best in 0..=tokens.len() {
                let mut shapes = Shapes::new(tokens);
                let result = compare(
                    &settings,
                    cluster,
                    tokens,
                    &positions,
                    &mut shapes,
                    digit_bits(tokens),
                    Some(best),
                );
                if equal >= best {
                    // Could still win (or tie): must not be dropped for the best so far. It may be
                    // dropped for the threshold, which `None` does not distinguish; so check the
                    // threshold side separately
                    let mut shapes = Shapes::new(tokens);
                    let unbounded = compare(
                        &settings,
                        cluster,
                        tokens,
                        &positions,
                        &mut shapes,
                        digit_bits(tokens),
                        None,
                    );
                    assert_eq!(
                        result.map(|r| r.equal),
                        unbounded.map(|r| r.equal),
                        "{tokens:?} best {best}"
                    );
                } else if let Some(r) = result {
                    assert_eq!(r.equal, equal);
                }
            }
        }
    }
}

#[test]
fn early_rejection_at_the_threshold_boundary() {
    // n = 10, t = 0.7: exactly 7 equal passes, 6 does not. Equal positions placed last, so the
    // comparison walks through the differences first and the bound is tight on the way
    let cluster = pattern_cluster("a b c d e f g h i j", &[]);
    let positions: Vec<Position> = Vec::new();
    for (message, expect) in [
        ("x y z d e f g h i j", true),
        ("x y z w e f g h i j", false),
        ("a b c d e f g x y z", true),
        ("a b c d e f w x y z", false),
    ] {
        let tokens: Vec<&str> = message.split_whitespace().collect();
        for flexible in [true, false] {
            let settings = settings_with(0.7, flexible);
            let mut shapes = Shapes::new(&tokens);
            let result = compare(
                &settings,
                &cluster,
                &tokens,
                &positions,
                &mut shapes,
                digit_bits(&tokens),
                None,
            );
            let passed = result.is_some_and(|r| passes_threshold(&settings, &r, tokens.len()));
            assert_eq!(passed, expect, "{message} flexible={flexible}");
        }
    }
    // Thresholds the command line does not allow: no early rejection, the result is complete
    for t in [1.5, f64::NAN, f64::INFINITY] {
        let settings = settings_with(t, true);
        let tokens: Vec<&str> = "x y z w v f g h i j".split_whitespace().collect();
        let mut shapes = Shapes::new(&tokens);
        let result = compare(
            &settings,
            &cluster,
            &tokens,
            &positions,
            &mut shapes,
            digit_bits(&tokens),
            None,
        );
        assert_eq!(result.map(|r| r.equal), Some(5), "t={t}");
    }
}

#[test]
fn early_rejection_with_variable_differences_and_wildcards() {
    // Every difference has digits on both sides: variable, the relaxed threshold lets it pass
    // whatever t is, so nothing may be rejected early
    let cluster = pattern_cluster("1 2 3 4 5 6 7 8 9 10", &[]);
    let tokens: Vec<&str> = "11 12 13 14 15 16 17 18 19 20".split_whitespace().collect();
    let positions: Vec<Position> = Vec::new();
    for t in [0.5, 0.7, 1.0] {
        let settings = settings_with(t, true);
        let mut shapes = Shapes::new(&tokens);
        let result = compare(
            &settings,
            &cluster,
            &tokens,
            &positions,
            &mut shapes,
            digit_bits(&tokens),
            None,
        );
        assert!(
            result.is_some_and(|r| passes_threshold(&settings, &r, 10)),
            "t={t}"
        );
    }
    // Wildcards do not count as equal: a pattern of only wildcards passes nothing at t > 0 with
    // ordinary words, and is rejected early
    let all_wild = pattern_cluster("a b c d e f g h i j", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    let words: Vec<&str> = "k l m n o p q r s t".split_whitespace().collect();
    let settings = settings_with(0.7, true);
    let mut shapes = Shapes::new(&words);
    assert!(
        compare(
            &settings,
            &all_wild,
            &words,
            &positions,
            &mut shapes,
            digit_bits(&words),
            None
        )
        .is_none()
    );
    // ... but with t = 0 everything passes, and the full result comes back
    let settings = settings_with(0.0, true);
    let mut shapes = Shapes::new(&words);
    let r = compare(
        &settings,
        &all_wild,
        &words,
        &positions,
        &mut shapes,
        digit_bits(&words),
        None,
    )
    .unwrap();
    assert_eq!((r.equal, r.wildcards), (0, 10));
}

// ---- Reading the pattern's text only when it is needed

/// The comparison as it was before the pattern's text was read only when needed: every token of
/// the pattern is read from the text (`Cluster::tokens`), and whether the message's token has a
/// digit is looked at again for every cluster. Early rejection included: `compare` must give
/// exactly the same result, `None` included.
///
/// Keep it in this plain form and in step with `compare`. A change that only makes `compare`
/// faster leaves this function alone: checking such a change is what it is for. A change to what
/// `compare` decides (what counts as equal, as a digit or code-like difference, or when a
/// comparison is given up) is made here too, the same way, in the same commit.
fn compare_reading_every_token(
    settings: &Settings,
    cluster: &Cluster,
    tokens: &[&str],
    positions: &[Position],
    shapes: &mut Shapes<'_>,
    best_equal: Option<usize>,
) -> Option<Comparison> {
    let mut result = Comparison {
        equal: 0,
        wildcards: 0,
        variable_diffs: 0,
        digit_diffs: 0,
    };
    let len = tokens.len();
    for (i, (p, t)) in cluster.tokens().zip(tokens).enumerate() {
        let (digits, code) = if p == WILDCARD {
            result.wildcards += 1;
            (has_digit(t), shapes.shape(i).is_code())
        } else if p == *t {
            result.equal += 1;
            continue;
        } else {
            let shape_t = shapes.shape(i);
            (
                has_digit(p) && has_digit(t),
                shape_t.is_code() && token_shape(p) == shape_t,
            )
        };
        if digits {
            result.digit_diffs += 1;
        }
        if digits || code || positions.get(i).is_some_and(Position::is_variable) {
            result.variable_diffs += 1;
        }
        let remaining = len - (i + 1);
        if best_equal.is_some_and(|best| result.equal + remaining < best)
            || !may_pass(
                settings,
                result.equal,
                result.variable_diffs,
                remaining,
                len,
            )
        {
            return None;
        }
    }
    Some(result)
}

/// Tokens for the random comparisons: ordinary words, tokens with digits, code-like tokens of
/// every shape, multibyte letters, quotes and punctuation. Some pairs have the same length in
/// bytes but a different text (`ışık`/`ışıl`, `Ayşe`/`Ayse1`, `ORDER_A`/`ORDER_B`,
/// `1234`/`abcd`), so that the length check is put to the test.
const MIXED_TOKENS: &[&str] = &[
    "Payment",
    "received",
    "rejected",
    "ok",
    "GET",
    "a",
    "b",
    "12ms",
    "40ms",
    "user_42",
    "user_9",
    "node-7",
    "100",
    "101",
    "10:00:00",
    "a1b2",
    "1234",
    "abcd",
    "ORDER_CREATED",
    "ORDER_FAILED",
    "ORDER_A",
    "ORDER_B",
    "sendInvoice",
    "SendSms",
    "send_invoice",
    "App\\Jobs\\SendInvoice",
    "App\\Jobs\\SendSms",
    "[App\\Jobs\\X]",
    "payment.failed",
    "rates:update",
    "'price'",
    "\"dsn\"",
    "$product",
    "$order",
    "ışık",
    "ışıl",
    "Ayşe",
    "Ayse1",
    "İstanbul",
    "çalışma",
    "ğüşiöç",
    "٣٤",
    "(Ayşe,",
    "'Ali",
    "Veli'",
    ",",
    "<NUM>",
    "<*>",
    "x1",
];

/// A token with the same length in bytes as `token` but a different text: another token of the
/// list if there is one, otherwise letters or digits.
fn same_length_token(rng: &mut Rng, token: &str) -> String {
    let others: Vec<&str> = MIXED_TOKENS
        .iter()
        .copied()
        .filter(|other| other.len() == token.len() && *other != token)
        .collect();
    if !others.is_empty() && rng.below(2) == 0 {
        return others[rng.below(others.len())].to_string();
    }
    let fill = if rng.below(2) == 0 { "z" } else { "7" };
    let other = fill.repeat(token.len());
    if other != token {
        other
    } else {
        "y".repeat(token.len())
    }
}

/// A cluster of `len` tokens of `MIXED_TOKENS` (now and then a name with a digit), separated by
/// different kinds of whitespace, multibyte ones included; some positions are wildcards.
fn random_cluster(rng: &mut Rng, len: usize) -> Cluster {
    const SEPARATORS: [&str; 5] = [" ", "  ", "\t", "\u{a0}", "\u{3000}"];
    let tokens: Vec<String> = (0..len)
        .map(|_| {
            if rng.below(8) == 0 {
                format!("{}{}", name(rng.below(30)), rng.below(100))
            } else {
                MIXED_TOKENS[rng.below(MIXED_TOKENS.len())].to_string()
            }
        })
        .collect();
    let mut text = String::new();
    for (j, token) in tokens.iter().enumerate() {
        if j > 0 || rng.below(4) == 0 {
            text.push_str(SEPARATORS[rng.below(SEPARATORS.len())]);
        }
        text.push_str(token);
    }
    let wildcards: Vec<usize> = (0..len).filter(|_| rng.below(4) == 0).collect();
    pattern_cluster(&text, &wildcards)
}

/// A message token facing the pattern's token `p` (`None`: a wildcard, or beyond the pattern):
/// the same token, one of the same length with a different text, a name with a digit, an
/// unusual token, or any token of the list.
fn message_token(rng: &mut Rng, p: Option<&str>) -> String {
    let p = p.filter(|p| *p != WILDCARD);
    match (p, rng.below(10)) {
        (Some(p), 0..=3) => p.to_string(),
        (Some(p), 4..=5) => same_length_token(rng, p),
        (_, 6) => format!("{}{}", name(rng.below(30)), rng.below(100)),
        // Never produced by splitting a message, but `compare` must not care
        (_, 7) if rng.below(4) == 0 => [WILDCARD, ""][rng.below(2)].to_string(),
        _ => MIXED_TOKENS[rng.below(MIXED_TOKENS.len())].to_string(),
    }
}

/// Leaf positions for the flexible threshold: none (the threshold is off, or the leaf is not
/// counted yet), or about one per position, some of them variable.
fn random_positions(rng: &mut Rng, len: usize) -> Vec<Position> {
    if rng.below(3) == 0 {
        return Vec::new();
    }
    let count = (len + rng.below(3)).saturating_sub(1);
    (0..count)
        .map(|_| {
            if rng.below(5) == 0 {
                Position::Variable
            } else {
                Position::Values(HashSet::new())
            }
        })
        .collect()
}

#[test]
fn reading_the_pattern_text_only_when_needed_changes_no_comparison() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let all_settings: Vec<Settings> = [0.0, 0.3, 0.5, 0.7, 0.8, 1.0, 1.5, f64::NAN]
        .into_iter()
        .flat_map(|t| [settings_with(t, true), settings_with(t, false)])
        .collect();
    // Wildcard, equal, same length but different; with different lengths: a digit only in the
    // message's token, a digit in both, code-like of the same shape, code-like of another shape,
    // none of these (the pattern's text is not read). Then: differing positions after the 64th.
    let mut seen = [0usize; 8];
    let mut after_64 = 0;
    let (mut kept, mut rejected) = (0, 0);
    for case in 0..1_500 {
        let len = if case % 10 == 0 {
            60 + rng.below(11)
        } else {
            1 + rng.below(12)
        };
        let clusters: Vec<Cluster> = (0..3).map(|_| random_cluster(&mut rng, len)).collect();
        // The message follows the first cluster; now and then it has a token more or less, so
        // that comparing stops where the shorter side ends, as before
        let message_len = match rng.below(10) {
            0 => len + 1 + rng.below(2),
            1 => len.saturating_sub(1).max(1),
            _ => len,
        };
        let model: Vec<&str> = clusters[0].tokens().collect();
        let message: Vec<String> = (0..message_len)
            .map(|j| message_token(&mut rng, model.get(j).copied()))
            .collect();
        let tokens: Vec<&str> = message.iter().map(String::as_str).collect();
        let positions = random_positions(&mut rng, len);
        let digits = digit_bits(&tokens);
        for settings in &all_settings {
            // One `Shapes` per message, shared by the clusters, as in `most_similar`
            let mut shapes_before = Shapes::new(&tokens);
            let mut shapes_now = Shapes::new(&tokens);
            for cluster in &clusters {
                for best_equal in [None, Some(rng.below(message_len + 2))] {
                    let before = compare_reading_every_token(
                        settings,
                        cluster,
                        &tokens,
                        &positions,
                        &mut shapes_before,
                        best_equal,
                    );
                    let now = compare(
                        settings,
                        cluster,
                        &tokens,
                        &positions,
                        &mut shapes_now,
                        digits,
                        best_equal,
                    );
                    let fields = |c: Option<Comparison>| {
                        c.map(|c| (c.equal, c.wildcards, c.variable_diffs, c.digit_diffs))
                    };
                    assert_eq!(
                        fields(now),
                        fields(before),
                        "{tokens:?} / {} t={} flexible={} best={best_equal:?}",
                        cluster.text(),
                        settings.similarity_threshold,
                        settings.flexible_threshold
                    );
                    if now.is_some() {
                        kept += 1;
                    } else {
                        rejected += 1;
                    }
                }
            }
        }
        for cluster in &clusters {
            for (i, (p, &t)) in cluster.tokens().zip(&tokens).enumerate() {
                let shape_t = token_shape(t);
                let kind = if p == WILDCARD {
                    0
                } else if p == t {
                    1
                } else if p.len() == t.len() {
                    2
                } else if has_digit(t) && !has_digit(p) {
                    3
                } else if has_digit(t) {
                    4
                } else if shape_t.is_code() && token_shape(p) == shape_t {
                    5
                } else if shape_t.is_code() {
                    6
                } else {
                    7
                };
                seen[kind] += 1;
                if i >= 64 && p != t {
                    after_64 += 1;
                }
            }
        }
    }
    assert!(seen.iter().all(|&n| n >= 20), "{seen:?}");
    assert!(after_64 >= 20, "{after_64}");
    assert!(kept >= 1_000 && rejected >= 1_000, "{kept} {rejected}");
}

#[test]
fn digit_bits_cover_the_first_64_tokens() {
    let message: Vec<String> = (0..70)
        .map(|i| {
            if i % 3 == 0 {
                format!("{}{i}", name(i))
            } else {
                name(i)
            }
        })
        .collect();
    let tokens: Vec<&str> = message.iter().map(String::as_str).collect();
    let bits = digit_bits(&tokens);
    for (i, token) in tokens.iter().enumerate().take(64) {
        assert_eq!(bits & (1 << i) != 0, has_digit(token), "{i} {token}");
    }
    // The tokens after the 64th have no bit: they are looked at when needed
    assert_eq!(digit_bits(&tokens[..64]), bits);
    assert_eq!(digit_bits(&[]), 0);
    // Every bit, the last one (token 63) included; a 65th token changes nothing
    let all_digits: Vec<String> = (0..65).map(|i| format!("k{i}")).collect();
    let all_digits: Vec<&str> = all_digits.iter().map(String::as_str).collect();
    assert_eq!(digit_bits(&all_digits[..64]), u64::MAX);
    assert_eq!(digit_bits(&all_digits), u64::MAX);
    assert_eq!(digit_bits(&all_digits[..63]), u64::MAX >> 1);
}

#[test]
fn tokens_at_and_after_the_64th_are_compared_as_before() {
    // Indexes from 0: the digits of tokens 0–63 come from `digit_bits`, those of token 64 and
    // after are looked at directly, as before. The message's only digit, if any, is at `at`; the
    // other tokens are equal digit-free words. A threshold of 0 gives nothing up early, so every
    // position is compared.
    let settings = settings_with(0.0, true);
    let positions: Vec<Position> = Vec::new();
    let mut checked = 0;
    for len in [63, 64, 65, 66, 70] {
        for at in [0, 62, 63, 64, 65, 69].into_iter().filter(|&at| at < len) {
            // Pattern token, whether it is a wildcard, message token, digit difference expected
            for (pattern_token, wildcard, message_token, digit_diff) in [
                ("xyzw", true, "k42", true),     // a digit facing a wildcard
                ("xyzw", true, "word", false),   // no digit facing a wildcard
                ("p1234", false, "m5", true),    // digits on both sides, different lengths
                ("p12", false, "m34", true),     // digits on both sides, the same length
                ("p1234", false, "word", false), // a digit only in the pattern
                ("abcd", false, "m5", false),    // a digit only in the message
            ] {
                let mut pattern: Vec<String> = (0..len).map(name).collect();
                pattern[at] = pattern_token.to_string();
                let wildcards = if wildcard { vec![at] } else { Vec::new() };
                let cluster = pattern_cluster(&pattern.join(" "), &wildcards);
                let mut message: Vec<String> = (0..len).map(name).collect();
                message[at] = message_token.to_string();
                let tokens: Vec<&str> = message.iter().map(String::as_str).collect();
                let digits = digit_bits(&tokens);
                let expected_bits = if has_digit(message_token) && at < 64 {
                    1 << at
                } else {
                    0
                };
                assert_eq!(digits, expected_bits, "len {len}, token {at}");
                let fields = |c: Option<Comparison>| {
                    c.map(|c| (c.equal, c.wildcards, c.variable_diffs, c.digit_diffs))
                };
                let mut shapes = Shapes::new(&tokens);
                let now = fields(compare(
                    &settings,
                    &cluster,
                    &tokens,
                    &positions,
                    &mut shapes,
                    digits,
                    None,
                ));
                // None of these tokens is code-like: a digit difference is the only variable one
                let expected = (
                    len - 1,
                    usize::from(wildcard),
                    usize::from(digit_diff),
                    usize::from(digit_diff),
                );
                let case = format!("len {len}, token {at}: {pattern_token} / {message_token}");
                assert_eq!(now, Some(expected), "{case}");
                let mut shapes = Shapes::new(&tokens);
                let before = fields(compare_reading_every_token(
                    &settings,
                    &cluster,
                    &tokens,
                    &positions,
                    &mut shapes,
                    None,
                ));
                assert_eq!(now, before, "{case}");
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 120);
}
