#[cfg(test)]
mod tests {
    use crate::tools::map::utils::*;
    use crate::tools::map::*;
    use crate::types::Options;
    use scraper::Html;

    #[tokio::test]
    async fn test_map_all_links() {
        let html = r#"
            <html><body>
                <a href="/page1">Link 1</a>
                <a href="/page2">Link 2</a>
                <a href="https://other.com">Link 3</a>
            </body></html>
        "#;

        let urls = map_page(html, "https://example.com").await;
        assert_eq!(urls.len(), 3);
        assert!(urls.contains(&"https://example.com/page1".to_string()));
        assert!(urls.contains(&"https://example.com/page2".to_string()));
        assert!(urls.contains(&"https://other.com/".to_string()));
    }

    #[tokio::test]
    async fn test_map_filters_invalid_schemes() {
        let html = r#"
            <html><body>
                <a href="/valid">Valid</a>
                <a href="javascript:void(0)">Invalid</a>
                <a href="mailto:test@example.com">Invalid</a>
            </body></html>
        "#;

        let urls = map_page(html, "https://example.com").await;
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://example.com/valid");
    }

    #[tokio::test]
    async fn test_map_empty() {
        let html = "<html><body>No links</body></html>";
        let urls = map_page(html, "https://example.com").await;
        assert_eq!(urls.len(), 0);
    }

    #[tokio::test]
    async fn test_map_protocol_relative() {
        let html = r#"<html><body><a href="//cdn.example.com/image.jpg">Link</a></body></html>"#;
        let urls = map_page(html, "https://example.com").await;
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://cdn.example.com/image.jpg");
    }

    #[tokio::test]
    async fn test_map_fragment_anchor() {
        let html = r##"<html><body><a href="#section">Link</a></body></html>"##;
        let urls = map_page(html, "https://example.com/page").await;
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://example.com/page#section");
    }

    #[tokio::test]
    async fn test_map_relative_and_absolute() {
        let html = r#"
            <html><body>
                <a href="/relative">Link 1</a>
                <a href="https://example.com/absolute">Link 2</a>
                <a href="page3">Link 3</a>
            </body></html>
        "#;

        let urls = map_page(html, "https://example.com/base/").await;
        assert_eq!(urls.len(), 3);
        assert!(urls.contains(&"https://example.com/relative".to_string()));
        assert!(urls.contains(&"https://example.com/absolute".to_string()));
        assert!(urls.contains(&"https://example.com/base/page3".to_string()));
    }

    #[tokio::test]
    async fn test_map_invalid_base() {
        let html = r#"<html><body><a href="/page">Link</a></body></html>"#;
        let urls = map_page(html, "not-a-url").await;
        assert_eq!(urls.len(), 0);
    }

    // ========== map_children tests ==========

    #[test]
    fn test_map_body_siblings() {
        let html = r#"
            <html><body>
                <div><h3>Recipe 1</h3><p>Desc</p></div>
                <div><h3>Recipe 2</h3><p>Desc</p></div>
                <div><h3>Recipe 3</h3><p>Desc</p></div>
            </body></html>
        "#;

        let siblings = map_body_siblings(html, &Options::default());
        assert_eq!(siblings.len(), 3);
        assert!(siblings[0].contains("Recipe 1"));
        assert!(siblings[1].contains("Recipe 2"));
        assert!(siblings[2].contains("Recipe 3"));
    }

    #[test]
    fn test_map_sibling_link() {
        let siblings = vec![
            r#"<div><a href="/recipe/1">Recipe 1</a></div>"#.to_string(),
            r#"<div><a href="/recipe/2">Recipe 2</a></div>"#.to_string(),
        ];

        let urls = map_sibling_link(&siblings, "https://example.com", &Options::default());
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0], "https://example.com/recipe/1");
        assert_eq!(urls[1], "https://example.com/recipe/2");
    }

    #[test]
    fn test_map_sibling_link_multiple() {
        // Sibling with multiple links - should return first non-excluded
        let siblings = vec![r#"
            <div>
                <a href="/recipe/1">Main Link</a>
                <a href="/share">Share</a>
                <a href="/print">Print</a>
            </div>
        "#
        .to_string()];

        let urls = map_sibling_link(&siblings, "https://example.com", &Options::default());
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://example.com/recipe/1");
    }

    #[test]
    fn test_map_siblings() {
        // Need pattern with child elements (h3 + p) for sibling detection
        let html = r#"
            <html><body>
                <article>
                    <div><h3>Recipe 1</h3><p><a href="/recipe/1">View</a></p></div>
                    <div><h3>Recipe 2</h3><p><a href="/recipe/2">View</a></p></div>
                    <div><h3>Recipe 3</h3><p><a href="/recipe/3">View</a></p></div>
                </article>
            </body></html>
        "#;

        let urls = map_siblings(
            html,
            "https://example.com",
            &crate::types::Options::default(),
        );
        assert_eq!(urls.len(), 3);
        assert!(urls.contains(&"https://example.com/recipe/1".to_string()));
        assert!(urls.contains(&"https://example.com/recipe/2".to_string()));
        assert!(urls.contains(&"https://example.com/recipe/3".to_string()));
    }

    #[test]
    fn test_map_jsonld_itemlist() {
        let html = r##"
            <html>
            <head>
                <script type="application/ld+json">
                {
                    "@type": "ItemList",
                    "itemListElement": [
                        {"@type": "ListItem", "position": 1, "url": "/recipe/1"},
                        {"@type": "ListItem", "position": 2, "url": "/recipe/2"}
                    ]
                }
                </script>
            </head>
            <body></body>
            </html>
        "##;

        let doc = Html::parse_document(html);
        let itemlist = map_jsonld_itemlist_from_doc(&doc);
        assert_eq!(itemlist.len(), 1);
        assert_eq!(
            itemlist[0].get("@type").unwrap().as_str().unwrap(),
            "ItemList"
        );
    }

    #[test]
    fn test_map_itemlist_link_full_urls() {
        let html = r##"
            <script type="application/ld+json">
            {
                "@type": "ItemList",
                "itemListElement": [
                    {"@type": "ListItem", "url": "https://example.com/recipe/1"},
                    {"@type": "ListItem", "url": "https://example.com/recipe/2"}
                ]
            }
            </script>
        "##;

        let doc = Html::parse_document(html);
        let itemlist = map_jsonld_itemlist_from_doc(&doc);
        let urls = map_itemlist_link(&itemlist, &doc, "https://example.com", &Options::default());

        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0], "https://example.com/recipe/1");
        assert_eq!(urls[1], "https://example.com/recipe/2");
    }

    #[test]
    fn test_map_itemlist_link_anchors() {
        let html = r##"
            <html>
            <head>
                <script type="application/ld+json">
                {
                    "@type": "ItemList",
                    "itemListElement": [
                        {"@type": "ListItem", "url": "#recipe-1"},
                        {"@type": "ListItem", "url": "#recipe-2"}
                    ]
                }
                </script>
            </head>
            <body>
                <div id="recipe-1"><a href="https://site.com/choc-chip">Chocolate Chip</a></div>
                <div id="recipe-2"><a href="https://site.com/oatmeal">Oatmeal</a></div>
            </body>
            </html>
        "##;

        let doc = Html::parse_document(html);
        let itemlist = map_jsonld_itemlist_from_doc(&doc);
        let urls = map_itemlist_link(&itemlist, &doc, "https://example.com", &Options::default());

        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0], "https://site.com/choc-chip");
        assert_eq!(urls[1], "https://site.com/oatmeal");
    }

    #[test]
    fn test_map_itemlist_link_multiple() {
        // Anchor points to element with multiple links - should return first
        let html = r##"
            <html>
            <head>
                <script type="application/ld+json">
                {
                    "@type": "ItemList",
                    "itemListElement": [
                        {"@type": "ListItem", "url": "#recipe-1"}
                    ]
                }
                </script>
            </head>
            <body>
                <div id="recipe-1">
                    <a href="https://site.com/recipe">Main Link</a>
                    <a href="https://social.com/share">Share</a>
                </div>
            </body>
            </html>
        "##;

        let doc = Html::parse_document(html);
        let itemlist = map_jsonld_itemlist_from_doc(&doc);
        let urls = map_itemlist_link(&itemlist, &doc, "https://example.com", &Options::default());

        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://site.com/recipe");
    }

    #[test]
    fn test_map_itemlist() {
        let html = r##"
            <html>
            <head>
                <script type="application/ld+json">
                {
                    "@type": "ItemList",
                    "itemListElement": [
                        {"@type": "ListItem", "url": "#recipe-1"},
                        {"@type": "ListItem", "url": "https://direct.com/recipe-2"}
                    ]
                }
                </script>
            </head>
            <body>
                <div id="recipe-1"><a href="https://site.com/recipe-1">Recipe 1</a></div>
            </body>
            </html>
        "##;

        let urls = map_itemlist(
            html,
            "https://example.com",
            &crate::types::Options::default(),
        );
        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"https://site.com/recipe-1".to_string()));
        assert!(urls.contains(&"https://direct.com/recipe-2".to_string()));
    }

    #[tokio::test]
    async fn test_map_children() {
        let html = r##"
            <html>
            <head>
                <script type="application/ld+json">
                {
                    "@type": "ItemList",
                    "itemListElement": [
                        {"@type": "ListItem", "url": "#recipe-1"},
                        {"@type": "ListItem", "url": "#recipe-2"}
                    ]
                }
                </script>
            </head>
            <body>
                <article>
                    <div id="recipe-1"><a href="/choc-chip">Chocolate Chip</a></div>
                    <div id="recipe-2"><a href="/oatmeal">Oatmeal</a></div>
                </article>
            </body>
            </html>
        "##;

        let urls = map_children(html, "https://example.com").await;
        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"https://example.com/choc-chip".to_string()));
        assert!(urls.contains(&"https://example.com/oatmeal".to_string()));
    }

    #[tokio::test]
    pub async fn test_map_children_from_real_website_1() {
        let html = r###"
        <main> <div></div> <article><div><div> <h1>Spectacular Halloween Cocktails to Spook Your Guests</h1> <p>Enchanting Drinks Featuring Creepy Garnishes and Unusual Ingredients</p> </div> <div><div><div><div> <span>By</span> <div> <a href=\"https://www.thespruceeats.com/colleen-graham-758955\" rel=\"nocaes\">Colleen Graham</a> <div> <div> <div> <div> <img width=\"200\" alt=\"Photo of Colleen Graham\" height=\"200\"> </div> </div> <div> <a rel=\"nocaes\" href=\"https://www.thespruceeats.com/colleen-graham-758955\">Colleen Graham</a> </div> <div> <ul> <li> <a rel=\"noopener nocaes\" target=\"_blank\" href=\"https://www.facebook.com/ColleensDrinkStudio\"> </a> </li> <li> <a rel=\"noopener nocaes\" target=\"_blank\" href=\"https://twitter.com/cocktailsguide\"> </a> </li> <li> <a target=\"_blank\" rel=\"noopener nocaes\" href=\"https://www.pinterest.com/cocktailsguide/\"> </a> </li> <li> <a target=\"_blank\" href=\"http://www.scdrinkstudio.com/\" rel=\"noopener nofollow nocaes\"> </a> </li> </ul> </div> <div> Writer and cocktail book author Colleen Graham is a seasoned mixologist who loves sharing her knowledge of spirits and passion for preparing drinks. </div> </div> <div> <span>Learn about The Spruce Eats'</span> <a href=\"/about-us-4776236#toc-editorial-guidelines\" rel=\"nocaes\">Editorial Process</a> </div> </div></div> </div> <div>Updated on 06/23/25</div></div> </div></div> </div><div><div></div> <div><div data-bgset=\"\"></div> <div><div></div> <button><span>Close</span> </button></div></div> <figure> <div> <div> <img sizes=\"750px\" alt=\"Black Widow Cocktail\" src=\"https://www.thespruceeats.com/thmb/RhpEpxyZy5wivA9kH3poaeW6aGY=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/black-widow-recipe-761008-hero-01-5c8801c7c9e77c0001a3e5c9.jpg\" height=\"3996\" width=\"5328\" srcset=\"https://www.thespruceeats.com/thmb/DONVESyHIOQmrQox-jycGosggqI=/750x0/filters:no_upscale():max_bytes(150000):strip_icc()/black-widow-recipe-761008-hero-01-5c8801c7c9e77c0001a3e5c9.jpg 750w\"> </div> </div> <figcaption> <span><p>The Spruce Eats</p></span> </figcaption></figure> <div><div><p> Halloween cocktails are creepy—sometimes gimmicky—and always fun to mix up. These thirst-quenching beverages are sure to add an extra spooky touch to your party and they're easy to make. You'll shake or stir these Halloween-worthy drinks like any other cocktail recipe, but many include cool special effects. From pumpkin-like garnishes to blood-red layers, these show-stopping and delicious cocktails and shots will both charm and frighten your guests. </p></div></div> <div><div><div><ul><li><div> <div> <span> </span> <span> 01 </span> <span>of 13</span> <span> </span> </div> </div> <div><span></span><h2> <a href=\"https://www.thespruceeats.com/jack-o-lantern-cocktail-recipe-759441\" rel=\"nocaes\">Jack-O-Lantern</a> </h2> <figure> <div> <div> <img height=\"914\" width=\"1371\" srcset=\"https://www.thespruceeats.com/thmb/2Tx-PKTeGK1RGJkypJk8SVG0_mA=/750x0/filters:no_upscale():max_bytes(150000):strip_icc()/jackolantern-level-example-6e53b034385543bf86de2a24984a4c26.jpg 750w\" alt=\"jack o'lantern cocktail hero image\" sizes=\"750px\" src=\"https://www.thespruceeats.com/thmb/iIFmVVHEPZTKDzVcSS2gSGnrNcw=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/jackolantern-level-example-6e53b034385543bf86de2a24984a4c26.jpg\"> </div> </div> <figcaption> <span><p>The Spruce Eats / Madhumita Sathishkumar</p></span> </figcaption></figure> <p> Several Halloween drink recipes use the name Jack-o'-lantern, yet few are as simple or eye-catching as this one. While it's not a pumpkin-flavored cocktail, it certainly looks like one. In this glass, you'll find a pleasant mix of cognac, orange liqueur, and orange juice topped with ginger ale. The Halloween-worthy garnish is what takes it from ordinary to extraordinary, and all you need is an orange and lime. </p> <div><a href=\"https://www.thespruceeats.com/halloween-drinks-cocktails-4162247\" rel=\"nocaes\"><span>Halloween Drinks &amp; Cocktails</span> <img width=\"420\" height=\"280\" alt=\"Halloween drinks and cocktail recipes cropped banner\"> </a></div></div> <div></div></li> <li><div> <div> <span> </span> <span> 02 </span> <span>of 13</span> <span> </span> </div> </div> <div><span></span><h2> <a rel=\"nocaes\" href=\"https://www.thespruceeats.com/halloween-hpnotist-recipe-761076\">Halloween Hypnotist</a> </h2> <figure> <div> <div> <img width=\"5971\" alt=\"Halloween Hypnotist Cocktail\" height=\"3970\"> </div> </div> <figcaption> <span><p> The Spruce Eats</p></span> </figcaption></figure> <p> Dazzle your guests with a bewitching martini. The haunting, eerie glow of the Halloween Hypnotist is sure to do the trick! The vodka recipe is easy and fruity, requiring just three common ingredients: vodka, Hpnotiq, and lemon juice. The glow stick \"garnish\" completes the effect spectacularly. </p></div> <div></div></li> <li><div> <div> <span> </span> <span> 03 </span> <span>of 13</span> <span> </span> </div> </div> <div><span></span><h2> <a href=\"https://www.thespruceeats.com/mad-eye-martini-recipe-761104\" rel=\"nocaes\">Mad Eye Martini</a> </h2> <figure> <div> <div> <img height=\"3944\" alt=\"Mad eye martini recipe\" width=\"5079\"> </div> </div> <figcaption> <span><p>The Spruce Eats / Julia Hartbeck</p></span> </figcaption></figure> <p> This gruesome cocktail has a beautiful pale blue color, a signature of Hpnotiq, and its flavor is as pleasant as can be with a delicate lychee accent. Creating the creepy garnish is quite easy and may take a bit of practice to perfect, but the membrane-like look of the lychee fruit is the perfect base. </p></div> <div></div></li> <li><div> <div> <span> </span> <span> 04 </span> <span>of 13</span> <span> </span> </div> </div> <div><span></span><h2> <a href=\"https://www.thespruceeats.com/blood-and-sand-cocktail-recipe-761336\" rel=\"nocaes\">Blood and Sand</a> </h2> <figure> <div> <div> <img width=\"3000\" height=\"2000\" alt=\"Blood and Sand Cocktail\"> </div> </div> <figcaption> <span><p>The Spruce Eats / Mateja Kobescak</p></span> </figcaption></figure> <p> Go old-school with an impressive variation on the Scotch Manhattan. In the Blood and Sand, you'll add a splash of cherry brandy and orange juice to the popular whisky-vermouth combination. This classic cocktail is a winner for any occasion, but its name makes it a perfect fit for Halloween. </p></div> <div><div><div>Continue to 5 of 13 below </div> <div> <div></div> </div></div> </div> <div></div></li> <li><div> <div> <span> </span> <span> 05 </span> <span>of 13</span> <span> </span> </div> </div> <div><span></span><h2> <a rel=\"nocaes\" href=\"https://www.thespruceeats.com/vampire-kiss-martini-recipe-761200\">Vampire Kiss Martini</a> </h2> <figure> <div> <div> <img width=\"3242\" height=\"2162\" alt=\"Vampire Kiss Champagne Cocktail\"> </div> </div> <figcaption> <span><p>The Spruce Eats / Julia Hartbeck</p></span> </figcaption></figure> <p> The Vampire Kiss Martini is elegant, sparkling, and you don't need a cocktail shaker to make it. Everyone will enjoy this tasty concoction of vodka, black raspberry liqueur, and Champagne, while the bloody red rim adds a frightful twist. You can also drop wax vampire teeth into the glass to give guests a special surprise. </p></div> <div></div></li> <li><div> <div> <span> </span> <span> 06 </span> <span>of 13</span> <span> </span> </div> </div> <div><span></span><h2> <a rel=\"nocaes\" href=\"https://www.thespruceeats.com/fright-night-in-the-grove-cocktail-760774\">Fright Night in the Grove</a> </h2> <figure> <div> <div> <img alt=\"Friday night in the grove cocktail recipe\" width=\"6075\" height=\"4050\"> </div> </div> <figcaption> <span><p>The Spruce Eats</p></span> </figcaption></figure> <p> Shock your guests by serving Jägermeister and tequila together in style. The fright night in the grove is easily made with simple syrup and grapefruit juice. It's a devilish drink and a new way to enjoy these two notorious spirits. </p></div> <div></div></li> <li><div> <div> <span> </span> <span> 07 </span> <span>of 13</span> <span> </span> </div> </div> <div><span></span><h2> <a rel=\"nocaes\" href=\"https://www.thespruceeats.com/frog-in-a-blender-recipe-761055\">Frog in a Blender</a> </h2> <figure> <div> <div> <img width=\"6016\" height=\"4000\" alt=\"Frog in a blender cocktail\"> </div> </div> <figcaption> <span><p> The Spruce Eats</p></span> </figcaption></figure> <p> Admittedly, some drinks are more gimmick than substance, and the Frog in a Blender is one of those. The concept behind this vodka-cranberry slushie is hard to beat. The trick is to avoid blending it as fine as a margarita, so all the green bits of lime remain chunky to create the illusion of a witch's brew. </p></div> <div></div></li> <li><div> <div> <span> </span> <span> 08 </span> <span>of 13</span> <span> </span> </div> </div> <div><span></span><h2> <a rel=\"nocaes\" href=\"https://www.thespruceeats.com/skeleton-key-cocktail-recipe-761383\">Skeleton Key</a> </h2> <figure> <div> <div> <img width=\"5713\" height=\"3983\" alt=\"Skeleton key cocktail recipe\"> </div> </div> <figcaption> <span><p>The Spruce Eats / Julia Hartbeck</p></span> </figcaption></figure> <p> When you're looking for a bloody good drink that will entertain and refresh, the Skeleton Key is a great choice. This unique bourbon cocktail includes elderflower and ginger beer with a simple bloody garnish. Bottle of bitters be gone! </p></div> <div><div><div>Continue to 9 of 13 below </div> <div> <div></div> </div></div> </div> <div></div></li> <li><div> <div> <span> </span> <span> 09 </span> <span>of 13</span> <span> </span> </div> </div> <div><span></span><h2> <a href=\"https://www.thespruceeats.com/black-widow-recipe-761008\" rel=\"nocaes\">Black Widow</a> </h2> <figure> <div> <div> <img alt=\"Black Widow Cocktail\" height=\"3996\" width=\"5328\"> </div> </div> <figcaption> <span><p>The Spruce Eats</p></span> </figcaption></figure> <p> Dark and mysterious, the Black Widow is a Halloween-inspired twist on a vodka cranberry. To pull it off, you'll need to find Blavod or make black vodka from scratch. </p></div> <div></div></li> <li><div> <div> <span> </span> <span> 10 </span> <span>of 13</span> <span> </span> </div> </div> <div><span></span><h2> <a rel=\"nocaes\" href=\"https://www.thespruceeats.com/ghostbuster-cocktail-recipe-759668\">Ghostbuster</a> </h2> <figure> <div> <div> <img alt=\"Ghostbuster cocktail\" height=\"3955\" width=\"5614\"> </div> </div> <figcaption> <span><p>The Spruce Eats / Julia Hartbeck</p></span> </figcaption></figure> <p> When you mix up the Ghostbuster, you'll find an apparition floating around in your glass. The recipe is easy and results in a green martini with a peachy melon flavor that everyone will die for. What's floating inside? Nothing more than a white spirit that you probably already have in your bar. </p> <div><a rel=\"nocaes\" href=\"https://www.thespruceeats.com/stock-your-bar-for-a-party-760394\"><span>How to Stock Your Bar for a Party</span> <img width=\"420\" alt=\"Pink Lady Cocktail recipe ingredients\" height=\"280\"> </a></div></div> <div></div></li> <li><div> <div> <span> </span> <span> 11 </span> <span>of 13</span> <span> </span> </div> </div> <div><span></span><h2> <a rel=\"nocaes\" href=\"https://www.thespruceeats.com/zombie-cocktail-recipe-761643\">Zombie</a> </h2> <figure> <div> <div> <img width=\"5472\" height=\"3648\" alt=\"Zombie Cocktail Recipe\"> </div> </div> <figcaption> <span><p>The Spruce Eats</p></span> </figcaption></figure> <p> Many cocktail recipes are named for things that go bump in the night, and a favorite among them is the classic Zombie, which is slightly different from the Zombie Punch. Both are old-school tropical cocktails, and either one of these fruit-filled, rum-heavy drinks will keep the party going all night. </p></div> <div></div></li> <li><div> <div> <span> </span> <span> 12 </span> <span>of 13</span> <span> </span> </div> </div> <div><span></span><h2> <a rel=\"nocaes\" href=\"https://www.thespruceeats.com/wolf-bite-shot-recipe-759565\">Wolf Bite</a> </h2> <figure> <div> <div> <img height=\"1000\" width=\"1500\" alt=\"Wolf Bite Shot\"> </div> </div> <figcaption> <span>The absinthe gives this Wolf Bite shooter its bite.</span> <span><p>The Spruce Eats / S&amp;C Design Studios</p></span> </figcaption></figure> <p> Treat your guests to a round of Halloween shots and serve up the memorable Wolf Bite. Like a mad scientist's experiment gone wrong, this fun absinthe and melon liqueur shooter—complete with a blood-red layer—needs to be seen before it goes down. </p></div> <div><div><div>Continue to 13 of 13 below </div> <div> <div></div> </div></div> </div> <div></div></li> <li><div> <div> <span> </span> <span> 13 </span> <span>of 13</span> <span> </span> </div> </div> <div><span></span><h2> <a href=\"https://www.thespruceeats.com/candy-corn-shooter-recipe-759614\" rel=\"nocaes\">Candy Corn Shot</a> </h2> <figure> <div> <div> <img height=\"4016\" alt=\"Candy corn shooter recipe\" width=\"6016\"> </div> </div> <figcaption> <span><p>The Spruce Eats / Julia Hartbeck </p></span> </figcaption></figure> <p> The key to the \"candy corn\" effect is layering the ingredients according to their specific gravity. Pouring the gold-colored Galliano, then orange curaçao, and topping it off with cream creates the same distinct striping as the classic Halloween candy. </p></div> <div></div></li></ul></div> <div><div><a rel=\"nocaes\" href=\"https://www.thespruceeats.com/sherbet-punch-non-alcoholic-760376\"><span>Non Alcoholic Sherbet Punch </span></a></div></div></div> <div></div> <div><div>Explore More:</div> <ul><li><a href=\"https://www.thespruceeats.com/food-by-occasion-season-4162319\" rel=\"nocaes\"><span>Recipes by Occasion</span></a></li> <li><a rel=\"nocaes\" href=\"https://www.thespruceeats.com/halloween-foods-4162250\"><span>Halloween Recipes</span></a></li> <li><a href=\"https://www.thespruceeats.com/halloween-drinks-cocktails-4162247\" rel=\"nocaes\"><span>Halloween Drinks</span></a></li></ul></div></div> </div><div><div><div><div><div><div><div> <div></div> </div></div></div></div> <div><div><div><div> <div></div> </div></div></div></div> <div><div><div><div> <div></div> </div></div></div></div> <div><div><div><div> <div></div> </div></div></div></div> <div><div><div><div> <div></div> </div></div></div></div> <div><div><div><div> <div></div> </div></div></div></div> <div><div><div><div> <div></div> </div></div></div></div></div></div> </div></article> <div><div> <div></div> </div> <div><div><div><div><div><a href=\"https://www.thespruceeats.com/fun-halloween-shots-4173410\"> <div><div> <img alt=\"Wolf Bite Shot\" width=\"300\" height=\"225\"> </div> </div> <div> <div> <div></div> <span> <span> 14 Hauntingly Fun Halloween Shots </span> </span> </div> <div> </div> </div> </a></div> <div><a href=\"https://www.thespruceeats.com/halloween-hpnotist-recipe-761076\"> <div><div> <img alt=\"Halloween Hpnotist Cocktail\" width=\"300\" height=\"225\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> The Halloween Hpnotist </span> </span> </div> <div><span> <span> 3 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div> <div><a href=\"https://www.thespruceeats.com/skeleton-key-cocktail-recipe-761383\"> <div><div> <img width=\"300\" alt=\"Two glasses with a Skeleton key cocktail in them \" height=\"225\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> Skeleton Key Cocktail </span> </span> </div> <div><span> <span> 3 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div> <div><a href=\"https://www.thespruceeats.com/donq-bloody-rum-punch-760454\"> <div><div> <img height=\"225\" alt=\"Don Q bloody rum punch recipe\" width=\"300\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> Bloody Rum Punch for Halloween </span> </span> </div> <div><span> <span> 10 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div> <div><a href=\"https://www.thespruceeats.com/fright-night-in-the-grove-cocktail-760774\"> <div><div> <img height=\"225\" alt=\"Friday night in the grove cocktail recipe\" width=\"300\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> Fright Night in the Grove Cocktail </span> </span> </div> <div><span> <span> 3 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div> <div><a href=\"https://www.thespruceeats.com/ghostbuster-cocktail-recipe-759668\"> <div><div> <img alt=\"Ghostbuster cocktail\" height=\"225\" width=\"300\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> The Ghostbuster Drink </span> </span> </div> <div><span> <span> 3 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div> <div><a href=\"https://www.thespruceeats.com/jack-o-lantern-cocktail-recipe-759441\"> <div><div> <img alt=\"Jack-O’-Lantern Cocktail\" height=\"225\" width=\"300\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> Jack-O’-Lantern Cocktail </span> </span> </div> <div><span> <span> 3 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div> <div><a href=\"https://www.thespruceeats.com/candy-corn-shooter-recipe-759614\"> <div><div> <img alt=\"Candy corn shooter recipe\" width=\"300\" height=\"225\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> Candy Corn Shot </span> </span> </div> <div><span> <span> 3 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div></div> <div> <div></div> </div> <div><div><a href=\"https://www.thespruceeats.com/pumpkin-martini-recipe-761145\"> <div><div> <img width=\"300\" height=\"225\" alt=\"A pumpkin martini garnished with a cinnamon stick\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> Pumpkin Martini </span> </span> </div> <div><span> <span> 5 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div> <div><a href=\"https://www.thespruceeats.com/wolf-bite-shot-recipe-759565\"> <div><div> <img width=\"300\" height=\"225\" alt=\"Wolf Bite shot\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> The Wolf Bite Absinthe Shot </span> </span> </div> <div><span> <span> 3 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div> <div><a href=\"https://www.thespruceeats.com/pumpkin-old-fashioned-recipe-761379\"> <div><div> <img height=\"225\" alt=\"pumpkin old fashioned cocktail\" width=\"300\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> Pumpkin Old-Fashioned </span> </span> </div> <div><span> <span> 5 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div> <div><a href=\"https://www.thespruceeats.com/apple-cider-old-fashioned-recipe-7559119\"> <div><div> <img alt=\"An apple cider old fashioned cocktail, garnished with a slice of apple, an orange peel, and a cinnamon stick\" width=\"300\" height=\"225\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> Apple Cider Old Fashioned </span> </span> </div> <div><span> <span> 20 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div> <div><a href=\"https://www.thespruceeats.com/halloween-lychee-eyeballs-5073596\"> <div><div> <img width=\"300\" height=\"225\" alt=\"Creepy Lychee Eyeballs for Halloween Cocktails and Drinks\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> Halloween Lychee Eyeballs Recipe </span> </span> </div> <div><span> <span> 60 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div> <div><a href=\"https://www.thespruceeats.com/zombie-punch-recipe-759868\"> <div><div> <img width=\"300\" alt=\"Classic Zombie Punch Tiki Cocktail\" height=\"225\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> Classic Zombie Punch </span> </span> </div> <div><span> <span> 3 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div> <div><a href=\"https://www.thespruceeats.com/rumchata-pumpkin-pie-martini-recipe-760971\"> <div><div> <img alt=\"RumChata Pumpkin Pie Martini\" width=\"300\" height=\"225\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> Pumpkin Pie Martini </span> </span> </div> <div><span> <span> 3 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div> <div><a href=\"https://www.thespruceeats.com/sherbet-punch-non-alcoholic-760376\"> <div><div> <img alt=\"Non Alcoholic Sherbet Punch in glasses and in a punch bowl \" width=\"300\" height=\"225\"> </div> <div> <button> </button> </div> </div> <div> <div> <div></div> <span> <span> Non Alcoholic Sherbet Punch </span> </span> </div> <div><span> <span> 5 mins </span> </span> <div> <span>Ratings</span> <div><div><span> </span><span> </span><span> </span><span> </span><span> </span></div> </div></div> </div> </div> </a></div></div> <div> <div></div> </div></div></div></div></div> </main>
        "###;

        let urls = map_children(html, "https://www.thespruceeats.com").await;

        assert!(
            urls.len() == 13,
            "Should have exactly 13 urls, got {}",
            urls.len()
        );
    }

    #[tokio::test]
    async fn test_map_children_from_real_website_2() {
        let html = r###"
        <main> <article> <div><p>I went down the Halloween cocktail rabbit hole the other day, and (wow!) there are some spooky, wild drinks out there. You might encounter <a href=\"http://www.delish.com/cooking/recipe-ideas/recipes/a44347/glowing-jell-o-shots-glow-party-foods/\">Glowing Jell-o Shots</a>, or <a href=\"http://www.latina.com/food/recipes/spooky-halloween-cocktails\">candy corn cocktails</a>, or even an <a href=\"http://www.countryliving.com/food-drinks/g3488/halloween-punch/?slide=2\">eyeball punch</a>. There's no shortage of cocktails you'd probably regret the next day - weird mixes of alcohols, overly sweet, lots of gummy worms in drinks, etc. So, I thought I'd do a quick round up of Halloween cocktails that were a bit less theme-y, ones that still had some ghoul and ghost, but also seemed delicious.</p> <p><strong>1. <a href=\"https://punchdrink.com/recipes/cardinale/\">Cardinale</a> - <em> (PUNCH) </em></strong><br> Blood red, and bone dry. <a href=\"https://punchdrink.com/recipes/cardinale/\">Get the recipe here</a>.</p> <p><img alt=\"Halloween Cocktails You're Less Likely to Regret\" loading=\"lazy\" fetchpriority=\"low\" src=\"https://images.101cookbooks.com/recipes/halloween-cocktails/cardinale-cocktail.jpg?w=620&amp;auto=format\" border=\"0\"></p> <p><strong>2. <a href=\"https://www.marthastewart.com/852648/blood-orange-cocktails\">Blood Orange Test Tubes</a> - <em> (Martha Stewart) </em></strong><br> I love the test tube delivery here, with the downloadable labels. <a href=\"https://www.marthastewart.com/852648/blood-orange-cocktails\">Get the recipe here</a>.</p> <p><img src=\"https://images.101cookbooks.com/recipes/halloween-cocktails/halloween-cocktail-phobias.jpg?w=620&amp;auto=format\" border=\"0\" alt=\"Halloween Cocktails You're Less Likely to Regret\" fetchpriority=\"low\" loading=\"lazy\"></p> <p><strong>3. <a href=\"http://www.delish.com/cooking/recipe-ideas/recipes/a44311/jekyll-gin-glowing-cocktails-glow-party-ideas/\">Jekyll Gin Glowing Cocktails</a> - <em> (Delish) </em></strong><br> This twist on a Gin Daisy glows in black light! Gin, grenadine, lemon juice, and tonic water. <a href=\"http://www.delish.com/cooking/recipe-ideas/recipes/a44311/jekyll-gin-glowing-cocktails-glow-party-ideas/\">Get the recipe here</a>.</p> <p><img alt=\"Halloween Cocktails You're Less Likely to Regret\" loading=\"lazy\" src=\"https://images.101cookbooks.com/recipes/halloween-cocktails/jekyll-gin-recipe.jpg?w=620&amp;auto=format\" fetchpriority=\"low\" border=\"0\"></p> <p><strong>4. <a href=\"http://www.foodandwine.com/recipes/pirate-mary\">Pirate Mary</a> - <em> (Food &amp; Wine) </em></strong><br> Yes to this cocktail. There's a nested recipe in the ingredient list, but it's no big deal (aside from sourcing the yellow tomato juice ;)...<a href=\"http://www.foodandwine.com/recipes/pirate-mary\">Get the recipe here</a>.</p> <p><img src=\"https://images.101cookbooks.com/recipes/halloween-cocktails/pirate-mary-halloween-cocktail.jpg?w=620&amp;auto=format\" alt=\"Halloween Cocktails You're Less Likely to Regret\" border=\"0\" loading=\"lazy\" fetchpriority=\"low\"></p> <p><strong>5. <a href=\"https://www.101cookbooks.com/archives/kombucha-dark-and-stormy-recipe.html\">Kombucha Dark &amp; Stormy</a> - <em> (101 Cookbooks) </em></strong><br> These are so delicious. Essentially, a twist on the classic cocktail make with strong ginger kombucha in place of ginger beer. A splash of rum, optional twist of lime, and you're good. <a href=\"https://www.101cookbooks.com/archives/kombucha-dark-and-stormy-recipe.html\">Get the recipe here</a>.</p> <p><img alt=\"Halloween Cocktails You're Less Likely to Regret\" loading=\"lazy\" border=\"0\" src=\"https://images.101cookbooks.com/recipes/halloween-cocktails/kombucha-dark-and-stormy.jpg?w=620&amp;auto=format\" fetchpriority=\"low\"></p> <p><strong>6. <a href=\"https://punchdrink.com/recipes/death-in-the-afternoon/\">Death in the Afternoon</a> - <em> (PUNCH) </em></strong><br> Two ingredients - absinthe and chilled Champagne. <a href=\"https://punchdrink.com/recipes/death-in-the-afternoon/\">Get the recipe here</a>.</p> <p><img loading=\"lazy\" fetchpriority=\"low\" src=\"https://images.101cookbooks.com/recipes/halloween-cocktails/Death-Afternoon.jpg?w=620&amp;auto=format\" border=\"0\" alt=\"Halloween Cocktails You're Less Likely to Regret\"></p> <p><strong>7. <a href=\"http://www.foodandwine.com/recipes/mothers-ruin-punch\">Mother's Ruin Punch</a> - <em> (Food &amp; Wine) </em></strong><br> If you're going to go the punch bowl route for your party, this looks gooood. Gin, grapefuit juice, and Champagne. <a href=\"http://www.foodandwine.com/recipes/mothers-ruin-punch\">Get the recipe here</a>.</p> <p><img border=\"0\" fetchpriority=\"low\" alt=\"Halloween Cocktails You're Less Likely to Regret\" loading=\"lazy\" src=\"https://images.101cookbooks.com/recipes/halloween-cocktails/mothers-ruin-punch.jpg?w=620&amp;auto=format\"></p> <div> <div> <div> <div> <div>101 Cookbooks Membership</div> <div> <div> <a href=\"/membership-account/membership-checkout.html?level=1#pmpro_level_cost\"><img alt=\"spice herb flower zest\" nopin=\"nopin\" width=\"100\" height=\"141\" loading=\"lazy\" fetchpriority=\"low\" src=\"https://images.101cookbooks.com/SPICE-HERB-COVER-100.png\"></a> <a href=\"/membership-account/membership-checkout.html?level=1\"> <img fetchpriority=\"low\" src=\"https://images.101cookbooks.com/WEEKNIGHT-EXPRESS-V2.100.png\" alt=\"weeknight express\" loading=\"lazy\" height=\"141\" nopin=\"nopin\" width=\"100\"></a> </div> <div> <p>Premium Ad-Free membership includes: <br> -Ad-free content <br> -Print-friendly recipes <br> -<i>Spice / Herb / Flower / Zest </i> recipe collection PDF<br> -<i>Weeknight Express</i> recipe collection PDF <br> -Surprise bonuses throughout the year <br> </p> </div> </div> </div> <div> <a href=\"/membership-account/membership-checkout.html?level=1#pmpro_level_cost\">Sign up here!</a> </div> </div> <div> <div> <a href=\"/membership-account/membership-checkout.html?level=1#pmpro_level_cost\"><img alt=\"spice herb flower zest\" height=\"141\" fetchpriority=\"low\" nopin=\"nopin\" loading=\"lazy\" src=\"https://images.101cookbooks.com/SPICE-HERB-COVER-100.png\" width=\"100\"></a></div> <div> <a href=\"/membership-account/membership-checkout.html?level=1#pmpro_level_cost\"><img fetchpriority=\"low\" alt=\"weeknight express\" nopin=\"nopin\" width=\"100\" height=\"141\" src=\"https://images.101cookbooks.com/WEEKNIGHT-EXPRESS-V2.100.png\" loading=\"lazy\"></a></div> </div> </div> </div> </div> </article> <div><h3>Related Recipes</h3><div><div><a href=\"https://www.101cookbooks.com/dark-and-stormy-recipe/\"><img alt=\"Kombucha Dark and Stormy\" src=\"https://images.101cookbooks.com/kombucha-dark-and-stormy-h.jpg?w=680&amp;auto=compress&amp;auto=format\" height=\"454\" fetchpriority=\"low\" width=\"680\" border=\"0\" loading=\"lazy\"></a></div> <div><h4><a href=\"https://www.101cookbooks.com/dark-and-stormy-recipe/\">Kombucha Dark and Stormy</a></h4><p>The perfect spicy, invigorating, Halloween cocktail. This is a twist on the classic Dark n' Stormy. Made with ginger-cayenne kombucha in place of traditional ginger beer. </p></div></div><div><div><a href=\"https://www.101cookbooks.com/fantastic-pumpkin-recipes/\"><img src=\"https://images.101cookbooks.com/great-pumpkin-recipes.jpg?w=680&amp;auto=compress&amp;auto=format\" border=\"0\" alt=\"10 Fantastic Pumpkin Recipes Worth Making this Fall\" height=\"454\" loading=\"lazy\" width=\"680\" fetchpriority=\"low\"></a></div> <div><h4><a href=\"https://www.101cookbooks.com/fantastic-pumpkin-recipes/\">10 Fantastic Pumpkin Recipes Worth Making this Fall</a></h4><p>The best pumpkin recipes currently on my radar for this fall. A curated list of recipes to have in rotation for peak pumpkin (and winter squash) season. Emphasis on dinner, emphasis on savory.</p></div></div><div><div><a href=\"https://www.101cookbooks.com/toasted-pumpkin-seeds/\"><img loading=\"lazy\" alt=\"Toasted Pumpkin Seeds: Three Ways\" src=\"https://images.101cookbooks.com/toasted-pumpkin-seeds-h.jpg?w=680&amp;auto=compress&amp;auto=format\" border=\"0\" width=\"680\" fetchpriority=\"low\" height=\"454\"></a></div> <div><h4><a href=\"https://www.101cookbooks.com/toasted-pumpkin-seeds/\">Toasted Pumpkin Seeds: Three Ways</a></h4><p>Toasted pumpkin seeds are the tiny, edible trophies you get for carving pumpkins. There are a couple of tricks to roasting perfect pumpkin seeds. </p></div></div><div><div><a href=\"https://www.101cookbooks.com/goth-hummus-recipe/\"><img border=\"0\" width=\"680\" loading=\"lazy\" height=\"454\" alt=\"Goth Hummus\" fetchpriority=\"low\" src=\"https://images.101cookbooks.com/goth-hummus-recipe-h.jpg?w=680&amp;auto=compress&amp;auto=format\"></a></div> <div><h4><a href=\"https://www.101cookbooks.com/goth-hummus-recipe/\">Goth Hummus</a></h4><p>It's basically just great hummus made with black chickpeas and black tahini. Perfect for a Halloween party! </p></div></div></div> <div> <div></div> <h4>Post Your Comment</h4> <div> <span> <small><a href=\"/7-halloween-cocktails/#respond\" rel=\"nofollow\">Cancel Reply</a></small></span> </div> <div></div> <div></div> </div> <div></div><h4>More Recipes</h4><div><div><a href=\"https://www.101cookbooks.com/whole_grain_recipes\">Whole Grain</a></div><div><a href=\"https://www.101cookbooks.com/wfpb\">WFPB</a></div><div><a href=\"https://www.101cookbooks.com/vegetarian_recipes\">Vegetarian Recipes</a></div><div><a href=\"https://www.101cookbooks.com/vegan-recipes/\">Vegan Recipes</a></div><div><a href=\"https://www.101cookbooks.com/soup-recipes/\">Soup Recipes</a></div><div><a href=\"https://www.101cookbooks.com/sides\">Side Dishes</a></div><div><a href=\"https://www.101cookbooks.com/sandwiches\">Sandwiches</a></div><div><a href=\"https://www.101cookbooks.com/salad-recipes/\">Salads</a></div><div><a href=\"https://www.101cookbooks.com/pasta-recipes/\">Pasta Recipes</a></div><div><a href=\"https://www.101cookbooks.com/quick_recipes\">Quick</a></div><div><a href=\"https://www.101cookbooks.com/main_courses\">Main Course</a></div><div><a href=\"https://www.101cookbooks.com/instant_pot_recipes\">Instant Pot</a></div><div><a href=\"https://www.101cookbooks.com/holiday_recipes\">Holiday</a></div><div><a href=\"https://www.101cookbooks.com/high_protein_recipes\">High Protein</a></div><div><a href=\"https://www.101cookbooks.com/gluten_free_recipes\">Gluten Free</a></div><div><a href=\"https://www.101cookbooks.com/drink_recipes\">Drinks</a></div><div><a href=\"https://www.101cookbooks.com/dinner_ideas\">Dinner Ideas</a></div><div><a href=\"https://www.101cookbooks.com/desserts\">Desserts</a></div><div><a href=\"https://www.101cookbooks.com/cookie-recipes/\">Cookies</a></div><div><a href=\"https://www.101cookbooks.com/chocolate_recipes\">Chocolate</a></div><div><a href=\"https://www.101cookbooks.com/breakfast_brunch\">Breakfast</a></div><div><a href=\"https://www.101cookbooks.com/baked_goods\">Baking</a></div><div><a href=\"https://www.101cookbooks.com/appetizers\">Appetizers</a></div><div><a href=\"https://www.101cookbooks.com/camping-recipes/\">Camping Recipes</a></div></div> <div><div><a border=\"0\" href=\"https://www.instagram.com/heidijswanson/\"><img src=\"https://images.101cookbooks.com/heidi-ico.jpg?auto=format\" fetchpriority=\"low\" alt=\"101cookbooks social icon\" nopin=\"nopin\" loading=\"lazy\"></a></div><div>Join my newsletter!<br> Weekly recipes and inspirations.</div> <div> </div><div><div>Follow Me:</div><div><a href=\"https://www.instagram.com/heidijswanson/\">Instagram</a></div><div><a href=\"https://www.tiktok.com/@heidijswanson/\">TikTok</a></div><div><a href=\"https://www.facebook.com/101cookbooks\">Facebook</a></div><div><a href=\"https://www.pinterest.com/heidiswanson/\">Pinterest</a></div></div></div> <h4>Popular Ingredients</h4><div><div><a href=\"https://www.101cookbooks.com/ingredient/avocado\">avocado</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/egg\">egg</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/herb\">herb</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/kale\">kale</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/lemon\">lemon</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/lentil\">lentil</a></div><div><a href=\"https://www.101cookbooks.com/how-to-cook-quinoa/\">quinoa</a></div><div><a href=\"https://www.101cookbooks.com/pasta-recipes/\">pasta</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/tomato\">tomato</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/turmeric\">turmeric</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/yogurt\">yogurt</a></div><div><a href=\"https://www.101cookbooks.com/zucchini/\">zucchini</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/arugula\">arugula</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/asparagus\">asparagus</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/basil\">basil</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/broccoli\">broccoli</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/buttermilk\">buttermilk</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/cauliflower\">cauliflower</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/chickpea\">chickpea</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/chocolate\">chocolate</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/curry\">curry</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/tempeh\">tempeh</a></div><div><a href=\"https://www.101cookbooks.com/ingredient/tofu\">tofu</a></div><div><a href=\"/ingredient.html\">ALL</a></div></div> <div></div> </main>
        "###;

        let urls = map_children(html, "https://www.101cookbooks.com").await;

        assert!(
            urls.len() == 7,
            "Should have exactly 7 urls, got {}",
            urls.len()
        );
    }
}
