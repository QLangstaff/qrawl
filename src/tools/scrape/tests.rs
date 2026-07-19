#![cfg(test)]
use crate::tools::extract::extract_schema_types;
use crate::tools::scrape::*;

// The boundary trap: a nested item must NOT leak its props to the parent,
// and a plain wrapper must NOT block the parent's props.
#[tokio::test]
async fn microdata_nested_item_and_plain_wrapper() {
    let html = r#"
            <div itemscope itemtype="https://schema.org/Recipe">
              <h1 itemprop="name">Avocado Soup</h1>
              <div>  <!-- plain wrapper: no itemprop, no itemscope -->
                <span itemprop="prepTime">PT15M</span>
                <span itemprop="cookTime">PT0M</span>
              </div>
              <div itemprop="author" itemscope itemtype="https://schema.org/Person">
                <span itemprop="name">Chef A</span>
              </div>
            </div>
        "#;
    let items = scrape_microdata(&html.into()).await;

    // Only the Recipe is top-level; the Person is nested (has itemprop).
    assert_eq!(items.len(), 1);
    let recipe = &items[0];
    assert_eq!(recipe["@type"], "Recipe");

    // Plain wrapper did NOT block these props.
    assert_eq!(recipe["prepTime"], "PT15M");
    assert_eq!(recipe["cookTime"], "PT0M");

    // The nested Person's `name` ("Chef A") must NOT leak into the Recipe:
    // Recipe.name is the single string "Avocado Soup", not an array.
    assert_eq!(recipe["name"], "Avocado Soup");

    // The nested item is preserved inside `author`.
    assert_eq!(recipe["author"]["@type"], "Person");
    assert_eq!(recipe["author"]["name"], "Chef A");
}

#[tokio::test]
async fn microdata_value_by_element_type() {
    let html = r#"
            <div itemscope itemtype="https://schema.org/Product">
              <meta itemprop="sku" content="ABC123">
              <a itemprop="url" href="https://ex.com/p">link text</a>
              <img itemprop="image" src="https://ex.com/i.jpg">
              <time itemprop="releaseDate" datetime="2024-01-01">Jan 1, 2024</time>
              <data itemprop="productID" value="42">forty-two</data>
              <span itemprop="name">Widget</span>
            </div>
        "#;
    let items = scrape_microdata(&html.into()).await;
    let p = &items[0];
    assert_eq!(p["sku"], "ABC123"); // <meta content>
    assert_eq!(p["url"], "https://ex.com/p"); // <a href>
    assert_eq!(p["image"], "https://ex.com/i.jpg"); // <img src>
    assert_eq!(p["releaseDate"], "2024-01-01"); // <time datetime>
    assert_eq!(p["productID"], "42"); // <data value>
    assert_eq!(p["name"], "Widget"); // text content
}

#[tokio::test]
async fn microdata_repeated_prop_becomes_array() {
    let html = r#"
            <div itemscope itemtype="https://schema.org/Recipe">
              <span itemprop="recipeIngredient">cucumber</span>
              <span itemprop="recipeIngredient">avocado</span>
              <span itemprop="recipeIngredient">lime</span>
            </div>
        "#;
    let items = scrape_microdata(&html.into()).await;
    assert_eq!(
        items[0]["recipeIngredient"],
        serde_json::json!(["cucumber", "avocado", "lime"])
    );
}

#[tokio::test]
async fn microdata_multiple_itemtype_tokens() {
    let html = r#"
            <div itemscope itemtype="https://schema.org/Product https://schema.org/IndividualProduct">
              <span itemprop="name">X</span>
            </div>
        "#;
    let items = scrape_microdata(&html.into()).await;
    assert_eq!(
        items[0]["@type"],
        serde_json::json!(["Product", "IndividualProduct"])
    );
}

#[tokio::test]
async fn microdata_anonymous_item_has_no_type() {
    let html = r#"<div itemscope><span itemprop="name">Anon</span></div>"#;
    let items = scrape_microdata(&html.into()).await;
    assert_eq!(items.len(), 1);
    assert!(items[0].get("@type").is_none());
    assert_eq!(items[0]["name"], "Anon");

    // extract_schema_types skips it (no @type).
    assert!(extract_schema_types(&items).is_empty());
}

#[tokio::test]
async fn microdata_short_type_handles_trailing_slash() {
    let html = r#"<div itemscope itemtype="https://schema.org/Recipe/"><span itemprop="name">R</span></div>"#;
    let items = scrape_microdata(&html.into()).await;
    assert_eq!(items[0]["@type"], "Recipe");
}

#[tokio::test]
async fn microdata_id_from_itemid() {
    let html = r#"<div itemscope itemtype="https://schema.org/Article" itemid="https://ex.com/a#1"><span itemprop="headline">H</span></div>"#;
    let items = scrape_microdata(&html.into()).await;
    assert_eq!(items[0]["@id"], "https://ex.com/a#1");
}

#[tokio::test]
async fn microdata_feeds_extract_schema_types() {
    let html = r#"
            <div itemscope itemtype="https://schema.org/Recipe"><span itemprop="name">R</span></div>
            <div itemscope itemtype="https://schema.org/Product"><span itemprop="name">P</span></div>
        "#;
    let items = scrape_microdata(&html.into()).await;
    let mut types = extract_schema_types(&items);
    types.sort();
    assert_eq!(types, vec!["Product", "Recipe"]);
}

// ===== RDFa (RDFa Lite) =====

#[tokio::test]
async fn rdfa_nested_item_and_plain_wrapper() {
    // The same boundary trap as Microdata, but the item marker is `typeof`.
    let html = r#"
            <div vocab="https://schema.org/" typeof="Recipe">
              <h1 property="name">Avocado Soup</h1>
              <div>  <!-- plain wrapper: no property, no typeof -->
                <span property="prepTime">PT15M</span>
                <span property="cookTime">PT0M</span>
              </div>
              <div property="author" typeof="Person">
                <span property="name">Chef A</span>
              </div>
            </div>
        "#;
    let items = scrape_rdfa(&html.into()).await;
    assert_eq!(items.len(), 1);
    let recipe = &items[0];
    assert_eq!(recipe["@type"], "Recipe");
    assert_eq!(recipe["prepTime"], "PT15M");
    assert_eq!(recipe["cookTime"], "PT0M");
    // The nested Person's name must NOT leak into the Recipe.
    assert_eq!(recipe["name"], "Avocado Soup");
    assert_eq!(recipe["author"]["@type"], "Person");
    assert_eq!(recipe["author"]["name"], "Chef A");
}

#[tokio::test]
async fn rdfa_content_attr_precedence() {
    let html = r#"
            <div typeof="https://schema.org/Article">
              <span property="headline" content="Real Headline">Display Text</span>
              <meta property="datePublished" content="2024-01-01">
            </div>
        "#;
    let items = scrape_rdfa(&html.into()).await;
    // @content overrides element text (and is the only value for <meta>).
    assert_eq!(items[0]["headline"], "Real Headline");
    assert_eq!(items[0]["datePublished"], "2024-01-01");
}

#[tokio::test]
async fn rdfa_link_and_media_values() {
    let html = r#"
            <div typeof="https://schema.org/Product">
              <a property="url" href="https://ex.com/p">link</a>
              <img property="image" src="https://ex.com/i.jpg">
              <span property="name">Widget</span>
            </div>
        "#;
    let items = scrape_rdfa(&html.into()).await;
    assert_eq!(items[0]["url"], "https://ex.com/p");
    assert_eq!(items[0]["image"], "https://ex.com/i.jpg");
    assert_eq!(items[0]["name"], "Widget");
}

#[tokio::test]
async fn rdfa_repeated_property_array_and_multiple_typeof() {
    let html = r#"
            <div typeof="https://schema.org/Product https://schema.org/IndividualProduct">
              <span property="keywords">a</span>
              <span property="keywords">b</span>
            </div>
        "#;
    let items = scrape_rdfa(&html.into()).await;
    assert_eq!(
        items[0]["@type"],
        serde_json::json!(["Product", "IndividualProduct"])
    );
    assert_eq!(items[0]["keywords"], serde_json::json!(["a", "b"]));
}

#[tokio::test]
async fn rdfa_short_type_handles_curie_and_iri() {
    let curie =
        scrape_rdfa(&r#"<div typeof="schema:Recipe"><span property="name">R</span></div>"#.into())
            .await;
    assert_eq!(curie[0]["@type"], "Recipe");
    let iri = scrape_rdfa(
        &r#"<div typeof="https://schema.org/Recipe"><span property="name">R</span></div>"#.into(),
    )
    .await;
    assert_eq!(iri[0]["@type"], "Recipe");
}

#[tokio::test]
async fn rdfa_id_from_resource_and_feeds_extract_schema_types() {
    let html = r#"<div typeof="https://schema.org/Article" resource="https://ex.com/a"><span property="headline">H</span></div>"#;
    let items = scrape_rdfa(&html.into()).await;
    assert_eq!(items[0]["@id"], "https://ex.com/a");
    assert_eq!(extract_schema_types(&items), vec!["Article"]);
}

#[tokio::test]
async fn rdfa_chaining_without_typeof_is_a_pinned_limitation() {
    // DOCUMENTED LIMITATION (see utils.rs RDFa header): a `property` that
    // introduces its own subject via `resource` but lacks `typeof` is not
    // split into a separate entity — its props flatten onto the parent.
    // This pins that behavior so it's a known limitation, not a surprise.
    let html = r##"
            <div vocab="https://schema.org/" typeof="Recipe">
              <div property="author" resource="#chef">
                <span property="name">Chef A</span>
              </div>
            </div>
        "##;
    let items = scrape_rdfa(&html.into()).await;
    // The sub-resource's `name` flattens onto the Recipe (the limitation).
    assert_eq!(items[0]["name"], "Chef A");
}

// ===== Structured: all three encodings merged =====

#[tokio::test]
async fn structured_merges_all_three_encodings() {
    let html = r#"
            <html><head>
              <script type="application/ld+json">{"@type":"Article","headline":"H"}</script>
            </head><body>
              <div itemscope itemtype="https://schema.org/Recipe"><span itemprop="name">R</span></div>
              <div vocab="https://schema.org/" typeof="Product"><span property="name">P</span></div>
            </body></html>
        "#;
    let items = scrape_structured(&html.into()).await;
    assert_eq!(items.len(), 3);
    // Order: JSON-LD, then Microdata, then RDFa.
    assert_eq!(items[0]["@type"], "Article");
    assert_eq!(items[1]["@type"], "Recipe");
    assert_eq!(items[2]["@type"], "Product");

    let mut types = extract_schema_types(&items);
    types.sort();
    assert_eq!(types, vec!["Article", "Product", "Recipe"]);
}

#[tokio::test]
async fn microdata_empty_when_absent() {
    let items =
        scrape_microdata(&"<html><body><p>no microdata here</p></body></html>".into()).await;
    assert!(items.is_empty());
}

// ===== Microformats2 =====

#[tokio::test]
async fn mf2_basic_h_recipe() {
    let html = r#"
            <div class="h-recipe">
              <h1 class="p-name">Avocado Soup</h1>
              <span class="p-ingredient">cucumber</span>
              <span class="p-ingredient">avocado</span>
              <span class="p-yield">4 servings</span>
            </div>
        "#;
    let items = scrape_microformats(&html.into()).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["type"], serde_json::json!(["h-recipe"]));
    assert_eq!(
        items[0]["properties"]["name"],
        serde_json::json!(["Avocado Soup"])
    );
    assert_eq!(
        items[0]["properties"]["ingredient"],
        serde_json::json!(["cucumber", "avocado"])
    );
    assert_eq!(
        items[0]["properties"]["yield"],
        serde_json::json!(["4 servings"])
    );
}

#[tokio::test]
async fn mf2_nested_h_card_is_a_property_not_a_leak() {
    let html = r#"
            <div class="h-entry">
              <h1 class="p-name">My Post</h1>
              <span class="p-author h-card"><span class="p-name">Alice</span></span>
            </div>
        "#;
    let items = scrape_microformats(&html.into()).await;
    assert_eq!(items.len(), 1); // h-card is nested, not top-level
                                // The nested h-card's name must NOT leak into the h-entry.
    assert_eq!(
        items[0]["properties"]["name"],
        serde_json::json!(["My Post"])
    );
    let author = &items[0]["properties"]["author"][0];
    assert_eq!(author["type"], serde_json::json!(["h-card"]));
    assert_eq!(author["properties"]["name"], serde_json::json!(["Alice"]));
    assert_eq!(author["value"], "Alice"); // implied value of a nested property
}

#[tokio::test]
async fn mf2_property_value_rules() {
    let html = r#"
            <div class="h-entry">
              <a class="u-url" href="https://ex.com/post">permalink</a>
              <time class="dt-published" datetime="2024-06-01">June 1</time>
              <div class="e-content">Hello <b>world</b></div>
              <span class="p-name">Title</span>
            </div>
        "#;
    let items = scrape_microformats(&html.into()).await;
    let props = &items[0]["properties"];
    assert_eq!(props["url"], serde_json::json!(["https://ex.com/post"])); // u- → href
    assert_eq!(props["published"], serde_json::json!(["2024-06-01"])); // dt- → datetime
    assert_eq!(props["name"], serde_json::json!(["Title"])); // p- → text
    let content = &props["content"][0]; // e- → {html, value}
    assert_eq!(content["value"], "Hello world");
    assert!(content["html"].as_str().unwrap().contains("<b>world</b>"));
}

#[tokio::test]
async fn mf2_children_for_non_property_nested_roots() {
    let html = r#"
            <div class="h-feed">
              <h1 class="p-name">My Blog</h1>
              <div class="h-entry"><span class="p-name">Post 1</span></div>
              <div class="h-entry"><span class="p-name">Post 2</span></div>
            </div>
        "#;
    let items = scrape_microformats(&html.into()).await;
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0]["properties"]["name"],
        serde_json::json!(["My Blog"])
    );
    let children = items[0]["children"].as_array().unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0]["type"], serde_json::json!(["h-entry"]));
    assert_eq!(
        children[1]["properties"]["name"],
        serde_json::json!(["Post 2"])
    );
}

#[tokio::test]
async fn mf2_ignores_css_utility_classes() {
    // Tailwind/Bootstrap padding utilities share the `p-` prefix but have
    // no-letter names → must NOT become properties named "2"/"4".
    let html = r#"
            <div class="h-card">
              <span class="p-name p-2 mb-4">Alice</span>
              <div class="p-4"><span class="p-org">Acme</span></div>
            </div>
        "#;
    let items = scrape_microformats(&html.into()).await;
    let props = items[0]["properties"].as_object().unwrap();
    assert_eq!(items[0]["properties"]["name"], serde_json::json!(["Alice"]));
    assert_eq!(items[0]["properties"]["org"], serde_json::json!(["Acme"]));
    assert!(props.get("2").is_none());
    assert!(props.get("4").is_none());
}

#[tokio::test]
async fn mf2_h_prefix_utility_classes_are_not_roots() {
    // Tailwind height utilities share the `h-` prefix; only whitelisted mf2
    // vocabularies are roots, so these produce nothing.
    let html = r#"
            <div class="h-screen flex">
              <div class="h-full"><p class="text-lg">Just Tailwind</p></div>
            </div>
        "#;
    assert!(scrape_microformats(&html.into()).await.is_empty());
}

#[tokio::test]
async fn mf2_implied_properties_are_a_pinned_limitation() {
    // DOCUMENTED LIMITATION (see utils.rs mf2 header): implied name/url are
    // deferred, so a minimal h-card yields empty properties rather than
    // implying name="Alice" / url=href. This pins that behavior.
    let html = r#"<a class="h-card" href="https://alice.example">Alice</a>"#;
    let items = scrape_microformats(&html.into()).await;
    assert_eq!(items[0]["type"], serde_json::json!(["h-card"]));
    assert!(items[0]["properties"].as_object().unwrap().is_empty());
}

#[tokio::test]
async fn microformats_are_separate_from_structured() {
    let html = r#"
            <script type="application/ld+json">{"@type":"Article"}</script>
            <div class="h-card"><span class="p-name">Alice</span></div>
        "#;
    // `scrape_structured` is the native schema.org encodings ONLY (JSON-LD /
    // Microdata / RDFa). mf2 is a distinct vocabulary and is NOT folded in
    // here — unifying it is `extract_schema`'s job (a composition).
    let structured = scrape_structured(&html.into()).await;
    let types: Vec<&str> = structured
        .iter()
        .filter_map(|v| v["@type"].as_str())
        .collect();
    assert!(types.contains(&"Article"), "JSON-LD present: {types:?}");
    assert!(
        !types.contains(&"Person"),
        "mf2 must NOT leak into scrape_structured: {types:?}"
    );
    // The h-card is available separately as raw mf2.
    let mf = scrape_microformats(&html.into()).await;
    assert_eq!(mf[0]["type"], serde_json::json!(["h-card"]));
}

#[tokio::test]
async fn mf2_empty_when_absent() {
    assert!(
        scrape_microformats(&"<div class='just-css'>hi</div>".into())
            .await
            .is_empty()
    );
}

// ===== Microformats1 backcompat (mf1 → mf2) =====

#[tokio::test]
async fn mf1_nested_author_vcard_in_hentry() {
    // The whole machine at once: root-map (hentry→h-entry), per-vocab prop
    // (author→p-author), cross-vocab nesting (author span re-resolves to
    // Vcard, fn→p-name), and the boundary (fn must NOT leak to entry name).
    let html = r#"
            <div class="hentry">
              <span class="entry-title">Post</span>
              <span class="author vcard"><span class="fn">Alice</span></span>
            </div>
        "#;
    let items = scrape_microformats(&html.into()).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["type"], serde_json::json!(["h-entry"]));
    // entry-title → name; nested vcard's fn must NOT leak here.
    assert_eq!(items[0]["properties"]["name"], serde_json::json!(["Post"]));
    let author = &items[0]["properties"]["author"][0];
    assert_eq!(author["type"], serde_json::json!(["h-card"]));
    assert_eq!(author["properties"]["name"], serde_json::json!(["Alice"]));
    assert_eq!(author["value"], "Alice");
}

#[tokio::test]
async fn mf1_vcard_multi_class_properties() {
    let html = r#"
            <div class="vcard">
              <a class="fn url" href="https://alice.example">Alice</a>
              <img class="photo" src="https://alice.example/p.jpg" alt="">
              <span class="bday">1990-12-28</span>
            </div>
        "#;
    let items = scrape_microformats(&html.into()).await;
    let props = &items[0]["properties"];
    assert_eq!(items[0]["type"], serde_json::json!(["h-card"]));
    // One element, two classes → two properties (fn→p-name text, url→u-url href).
    assert_eq!(props["name"], serde_json::json!(["Alice"]));
    assert_eq!(props["url"], serde_json::json!(["https://alice.example"]));
    assert_eq!(
        props["photo"],
        serde_json::json!(["https://alice.example/p.jpg"])
    );
    assert_eq!(props["bday"], serde_json::json!(["1990-12-28"]));
}

#[tokio::test]
async fn mf1_hrecipe() {
    let html = r#"
            <div class="hrecipe">
              <h1 class="fn">Soup</h1>
              <span class="ingredient">cucumber</span>
              <span class="ingredient">avocado</span>
              <span class="yield">4 servings</span>
            </div>
        "#;
    let items = scrape_microformats(&html.into()).await;
    assert_eq!(items[0]["type"], serde_json::json!(["h-recipe"]));
    assert_eq!(items[0]["properties"]["name"], serde_json::json!(["Soup"]));
    assert_eq!(
        items[0]["properties"]["ingredient"],
        serde_json::json!(["cucumber", "avocado"])
    );
    assert_eq!(
        items[0]["properties"]["yield"],
        serde_json::json!(["4 servings"])
    );
}

#[tokio::test]
async fn mf1_hfeed_entries_become_children() {
    let html = r#"
            <div class="hfeed">
              <div class="hentry"><span class="entry-title">Post 1</span></div>
              <div class="hentry"><span class="entry-title">Post 2</span></div>
            </div>
        "#;
    let items = scrape_microformats(&html.into()).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["type"], serde_json::json!(["h-feed"]));
    let children = items[0]["children"].as_array().unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0]["type"], serde_json::json!(["h-entry"]));
    assert_eq!(
        children[1]["properties"]["name"],
        serde_json::json!(["Post 2"])
    );
}

#[tokio::test]
async fn mf1_nested_adr_does_not_leak_into_vcard() {
    // The reason adr/geo must be roots: their sub-properties stay in the
    // nested h-adr instead of leaking onto the h-card.
    let html = r#"
            <div class="vcard">
              <span class="fn">Acme</span>
              <span class="adr">
                <span class="locality">Portland</span>
                <span class="region">OR</span>
              </span>
            </div>
        "#;
    let items = scrape_microformats(&html.into()).await;
    assert_eq!(items[0]["properties"]["name"], serde_json::json!(["Acme"]));
    let adr = &items[0]["properties"]["adr"][0];
    assert_eq!(adr["type"], serde_json::json!(["h-adr"]));
    assert_eq!(
        adr["properties"]["locality"],
        serde_json::json!(["Portland"])
    );
    assert_eq!(adr["properties"]["region"], serde_json::json!(["OR"]));
    // The address parts must NOT appear on the h-card itself.
    assert!(items[0]["properties"].get("locality").is_none());
}

#[tokio::test]
async fn mf1_empty_geo_dropped_but_populated_geo_kept() {
    // A stray CSS `class="geo"` yields an empty h-geo → dropped.
    let empty = scrape_microformats(&r#"<div class="geo"></div>"#.into()).await;
    assert!(empty.is_empty());
    // A real geo with coordinates survives.
    let real = scrape_microformats(
            &r#"<div class="geo"><span class="latitude">45.5</span><span class="longitude">-122.6</span></div>"#.into(),
        )
        .await;
    assert_eq!(real.len(), 1);
    assert_eq!(
        real[0]["properties"]["latitude"],
        serde_json::json!(["45.5"])
    );
}

#[tokio::test]
async fn mf1_empty_vcard_is_not_dropped() {
    // The guard is scoped to geo/adr — a minimal vcard (a microformat-only
    // coinage) survives with empty properties, like the mf2 h-card.
    let items = scrape_microformats(&r#"<span class="vcard"></span>"#.into()).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["type"], serde_json::json!(["h-card"]));
}

#[tokio::test]
async fn mf1_dual_root_prefers_mf2() {
    // An element with both `h-card` (mf2) and `vcard` (mf1) → mf2 wins, so
    // the mf2-prefixed `p-name` is read and the bare `fn` is ignored.
    let html = r#"
            <div class="h-card vcard">
              <span class="p-name">A</span>
              <span class="fn">B</span>
            </div>
        "#;
    let items = scrape_microformats(&html.into()).await;
    assert_eq!(items[0]["type"], serde_json::json!(["h-card"]));
    assert_eq!(items[0]["properties"]["name"], serde_json::json!(["A"]));
}
