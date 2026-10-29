use super::shape::{Shape, token_shape};
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
