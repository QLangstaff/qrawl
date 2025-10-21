use crate::{services::Extractor, types::*};
use async_trait::async_trait;

/* ---------- Traits that impls.rs provides ---------- */

#[async_trait]
pub trait Fetcher: Send + Sync {
    fn fetch_blocking(&self, url: &str) -> crate::Result<String>;

    /// Async variant of fetch_blocking. Must be implemented by concrete types.
    async fn fetch_async(&self, url: &str) -> crate::Result<String>;

    /// Optional; concrete impls (like reqwest) can override.
    fn name(&self) -> &'static str {
        "fetcher"
    }
}

pub trait Scraper: Send + Sync {
    fn scrape(&self, url: &str, html: &str) -> crate::Result<PageExtraction>;
    /// Optional; concrete impls can override.
    fn name(&self) -> &'static str {
        "scraper"
    }
}

/* ---------- Engine options ---------- */

#[derive(Clone, Copy, Default)]
pub struct EngineOptions {
    pub max_children: usize,
}

/* ---------- Engine ---------- */

pub struct Engine<'a> {
    pub fetcher: &'a dyn Fetcher,
    pub scraper: &'a dyn Scraper,
    pub extractor: &'a dyn Extractor,
    pub opts: EngineOptions,
}

impl<'a> Engine<'a> {
    pub fn new(
        fetcher: &'a dyn Fetcher,
        scraper: &'a dyn Scraper,
        extractor: &'a dyn Extractor,
        opts: EngineOptions,
    ) -> Self {
        Self {
            fetcher,
            scraper,
            extractor,
            opts,
        }
    }

    pub fn extract(&self, url: &str) -> Result<ExtractionBundle> {
        // Phase 1: Fetch and scrape to get structured content
        let html = self.fetcher.fetch_blocking(url)?;
        let page = self.scraper.scrape(url, &html)?;

        // Phase 2: Extract parent/child relationships from structured content
        self.extractor.extract(page)
    }

    pub async fn extract_async(&self, url: &str) -> Result<ExtractionBundle> {
        // Phase 1: Fetch and scrape to get structured content
        let html = self.fetcher.fetch_async(url).await?;
        let page = self.scraper.scrape(url, &html)?;

        // Phase 2: Extract parent/child relationships from structured content
        self.extractor.extract_async(page).await
    }

    /// Search for content on a specific domain (synchronous)
    /// Uses SearchService to perform Google site search
    pub fn search_blocking(&self, domain: &str, query: &str) -> Result<Option<String>> {
        use crate::services::SearchService;

        let search_service = SearchService::new()?;
        search_service.search_site_blocking(domain, query)
    }

    /// Search for content on a specific domain (asynchronous)
    /// Uses SearchService to perform Google site search
    pub async fn search_async(&self, domain: &str, query: &str) -> Result<Option<String>> {
        use crate::services::SearchService;

        let search_service = SearchService::new()?;
        search_service.search_site_async(domain, query).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::{DefaultExtractor, DefaultScraper, ReqwestFetcher, SectionScopedScraper};

    /// Test case definition for URL extraction testing
    #[derive(Debug, Clone)]
    struct TestCaseSection {
        subtitle: &'static str,
        link: &'static str,
    }
    #[derive(Debug, Clone)]
    struct TestCase {
        url: &'static str,
        title: &'static str,
        sections: Vec<TestCaseSection>,
    }
    #[derive(Debug, Clone)]
    struct TestCaseSectionWithImage {
        subtitle: &'static str,
        link: &'static str,
        image: &'static str,
    }
    #[derive(Debug, Clone)]
    struct TestCaseWithImage {
        url: &'static str,
        title: &'static str,
        image: &'static str,
        sections: Vec<TestCaseSectionWithImage>,
    }

    /// Creates a test engine with all required components
    fn create_test_engine() -> (ReqwestFetcher, DefaultScraper, DefaultExtractor) {
        let fetcher = ReqwestFetcher::new().expect("Failed to create fetcher");
        let scraper = DefaultScraper::new();
        let extractor = DefaultExtractor::new();
        (fetcher, scraper, extractor)
    }

    /// Create test engine with section-scoped scraper for A/B testing
    fn create_section_scoped_test_engine(
    ) -> (ReqwestFetcher, SectionScopedScraper, DefaultExtractor) {
        let fetcher = ReqwestFetcher::new().expect("Failed to create fetcher");
        let scraper = SectionScopedScraper::new();
        let extractor = DefaultExtractor::new();
        (fetcher, scraper, extractor)
    }

    /// Helper function to print expected sections
    fn print_expected_sections(sections: &[TestCaseSection]) {
        println!("\n   Expected Sections ({}):", sections.len());
        for (i, expected) in sections.iter().enumerate() {
            println!("     {}.", i + 1);
            println!("        Subtitle: '{}'", expected.subtitle);
            println!("        Link: '{}'", expected.link);
        }
    }

    /// Helper function to print expected sections
    fn print_expected_sections_with_images(sections: &[TestCaseSectionWithImage]) {
        println!("\n   Expected Sections ({}):", sections.len());
        for (i, expected) in sections.iter().enumerate() {
            println!("     {}.", i + 1);
            println!("        Subtitle: '{}'", expected.subtitle);
            println!("        Link: '{}'", expected.link);
            println!("        Image: '{}'", expected.image);
        }
    }

    /// Helper function to print extracted sections
    fn print_extracted_sections(sections: &[ContentSection]) {
        println!("\n   Extracted Sections ({}):", sections.len());
        for (i, extracted) in sections.iter().enumerate() {
            let no_subtitle = "''".to_string();
            let subtitle = extracted.subtitle.as_ref().unwrap_or(&no_subtitle);

            let link_display = match extracted.links.as_ref() {
                Some(links) if links.len() == 1 => format!("'{}'", links[0].href),
                Some(links) if links.len() > 1 => {
                    let urls = links
                        .iter()
                        .map(|l| format!("'{}'", l.href))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{} links: [{}]", links.len(), urls)
                }
                Some(links) if links.is_empty() => "''".to_string(),
                None => "''".to_string(),
                _ => unreachable!(),
            };

            println!("     {}.", i + 1);
            println!("        Subtitle: '{}'", subtitle);
            println!("        Link: {}", link_display);
        }
    }

    fn print_extracted_sections_with_images(sections: &[ContentSection]) {
        println!("\n   Extracted Sections ({}):", sections.len());
        for (i, extracted) in sections.iter().enumerate() {
            let no_subtitle = "''".to_string();
            let subtitle = extracted.subtitle.as_ref().unwrap_or(&no_subtitle);

            let link_display = match extracted.links.as_ref() {
                Some(links) if links.len() == 1 => format!("'{}'", links[0].href),
                Some(links) if links.len() > 1 => {
                    let urls = links
                        .iter()
                        .map(|l| format!("'{}'", l.href))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{} links: [{}]", links.len(), urls)
                }
                Some(links) if links.is_empty() => "''".to_string(),
                None => "''".to_string(),
                _ => unreachable!(),
            };

            let image_display = match extracted.images.as_ref() {
                Some(images) if images.len() >= 1 => format!("'{}'", images[0].src),
                Some(images) if images.is_empty() => "''".to_string(),
                None => "''".to_string(),
                _ => unreachable!(),
            };

            println!("     {}.", i + 1);
            println!("        Subtitle: '{}'", subtitle);
            println!("        Link: {}", link_display);
            println!("        Image: {}", image_display);
        }
    }

    /// Tests extraction for a single URL and validates section count and title
    fn test_url_extraction(test_case: &TestCase, engine: &Engine) {
        println!("\n🧪 Testing {}", test_case.url);

        let result = engine
            .extract(test_case.url)
            .expect(&format!("Failed to extract from URL: {}", test_case.url));

        // Validate title
        let mut error_count = 0;

        let extracted_title = result
            .parent
            .main_content
            .title
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("");

        if extracted_title == test_case.title {
            println!("\n🟢 Title: '{}'", extracted_title);
        } else {
            error_count += 1;
            println!("\n❌ Title mismatch!");
            println!("   Expected: '{}'", test_case.title);
            println!("   Extracted: '{}'", extracted_title);
        }

        // Validate sections exist and count matches
        let extracted_sections = result.parent.main_content.sections.unwrap_or_else(Vec::new);

        if extracted_sections.len() != test_case.sections.len() {
            error_count += 1;
            println!("\n❌ Sections mismatch!");
            print_expected_sections(&test_case.sections);

            if extracted_sections.is_empty() {
                println!("\n   Extracted Sections (0):");
            } else {
                print_extracted_sections(&extracted_sections);
            }
        }

        // Print and validate each section's content if count matches
        if extracted_sections.len() == test_case.sections.len() {
            println!("\n🟢 Sections ({}):", extracted_sections.len());

            for (i, (extracted_section, expected_section)) in extracted_sections
                .iter()
                .zip(test_case.sections.iter())
                .enumerate()
            {
                println!("  {}.", i + 1);

                let extracted_subtitle = extracted_section
                    .subtitle
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or("");

                if extracted_subtitle == expected_section.subtitle {
                    println!("    🟢 Subtitle: '{}'", extracted_subtitle);
                } else {
                    error_count += 1;
                    println!("    ❌ Subtitle mismatch!");
                    println!("       Expected: '{}'", expected_section.subtitle);
                    println!("       Extracted: '{}'", extracted_subtitle);
                }

                let extracted_link = match extracted_section.links.as_ref() {
                    None => String::new(),
                    Some(links) if links.is_empty() => String::new(),
                    Some(links) if links.len() == 1 => links[0].href.clone(),
                    Some(links) => {
                        // Multiple links - format as array
                        format!(
                            "[{}]",
                            links
                                .iter()
                                .map(|l| format!("'{}'", l.href))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                };

                if extracted_link == expected_section.link {
                    println!("    🟢 Link: '{}'", extracted_link);
                } else {
                    error_count += 1;
                    println!("    ❌ Link mismatch!");
                    println!("       Expected Link: '{}'", expected_section.link);
                    if extracted_link.is_empty() {
                        println!("       Extracted 0 Links: ");
                    } else if extracted_link.starts_with('[') {
                        let count = extracted_link.matches(", ").count() + 1;
                        println!("       Extracted {} Links: {}", count, extracted_link);
                    } else {
                        println!("       Extracted Link: '{}'", extracted_link);
                    }
                    println!("extracted_section.links: {:#?}", extracted_section.links);
                }
            }
        }
        // Report if there were any errors
        if error_count > 0 {
            panic!(
                "FAILED - {} error{}!",
                error_count,
                if error_count == 1 { "" } else { "s" }
            );
        }

        println!("\nPASSED");
    }
    /// Tests extraction for a single URL and validates section count, title, and image
    fn test_url_extraction_with_image(test_case: &TestCaseWithImage, engine: &Engine) {
        println!("\n🧪 Testing {}", test_case.url);

        let result = engine
            .extract(test_case.url)
            .expect(&format!("Failed to extract from URL: {}", test_case.url));

        // Validate title
        let mut error_count = 0;

        let extracted_title = result
            .parent
            .main_content
            .title
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("");

        if extracted_title == test_case.title {
            println!("\n🟢 Title: '{}'", extracted_title);
        } else {
            error_count += 1;
            println!("\n❌ Title mismatch!");
            println!("   Expected: '{}'", test_case.title);
            println!("   Extracted: '{}'", extracted_title);
        }

        let extracted_image = match result.parent.main_content.images.as_ref() {
            None => "",
            Some(images) if images.is_empty() => "",
            Some(images) if images.len() >= 1 => &images[0].src,
            _ => unreachable!(),
        };

        if extracted_image == test_case.image {
            println!("\n🟢 Image: '{}'", extracted_image);
        } else {
            error_count += 1;
            println!("\n❌ Image mismatch!");
            println!("   Expected: '{}'", test_case.image);
            println!("   Extracted: '{}'", extracted_image);
        }

        // Validate sections exist and count matches
        let extracted_sections = result.parent.main_content.sections.unwrap_or_else(Vec::new);

        if extracted_sections.len() != test_case.sections.len() {
            error_count += 1;
            println!("\n❌ Sections mismatch!");
            print_expected_sections_with_images(&test_case.sections);

            if extracted_sections.is_empty() {
                println!("\n   Extracted Sections (0):");
            } else {
                print_extracted_sections_with_images(&extracted_sections);
            }
        }

        // Print and validate each section's content if count matches
        if extracted_sections.len() == test_case.sections.len() {
            println!("\n🟢 Sections ({}):", extracted_sections.len());

            for (i, (extracted_section, expected_section)) in extracted_sections
                .iter()
                .zip(test_case.sections.iter())
                .enumerate()
            {
                println!("  {}.", i + 1);

                let extracted_subtitle = extracted_section
                    .subtitle
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or("");

                if extracted_subtitle == expected_section.subtitle {
                    println!("    🟢 Subtitle: '{}'", extracted_subtitle);
                } else {
                    error_count += 1;
                    println!("    ❌ Subtitle mismatch!");
                    println!("       Expected: '{}'", expected_section.subtitle);
                    println!("       Extracted: '{}'", extracted_subtitle);
                }

                let extracted_link = match extracted_section.links.as_ref() {
                    None => String::new(),
                    Some(links) if links.is_empty() => String::new(),
                    Some(links) if links.len() == 1 => links[0].href.clone(),
                    Some(links) => {
                        // Multiple links - format as array
                        format!(
                            "[{}]",
                            links
                                .iter()
                                .map(|l| format!("'{}'", l.href))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                };

                if extracted_link == expected_section.link {
                    println!("    🟢 Link: '{}'", extracted_link);
                } else {
                    error_count += 1;
                    println!("    ❌ Link mismatch!");
                    println!("       Expected Link: '{}'", expected_section.link);
                    if extracted_link.is_empty() {
                        println!("       Extracted 0 Links: ");
                    } else if extracted_link.starts_with('[') {
                        let count = extracted_link.matches(", ").count() + 1;
                        println!("       Extracted {} Links: {}", count, extracted_link);
                    } else {
                        println!("       Extracted Link: '{}'", extracted_link);
                    }
                    println!("extracted_section.links: {:#?}", extracted_section.links);
                }

                let extracted_image = match extracted_section.images.as_ref() {
                    None => "",
                    Some(images) if images.is_empty() => "",
                    Some(images) if images.len() >= 1 => &images[0].src,
                    _ => unreachable!(),
                };

                if extracted_image == expected_section.image {
                    println!("    🟢 Image: '{}'", extracted_image);
                } else {
                    error_count += 1;
                    println!("    ❌ Image mismatch!");
                    println!("       Expected: '{}'", expected_section.image);
                    println!("       Extracted: '{}'", extracted_image);
                }
            }
        }
        // Report if there were any errors
        if error_count > 0 {
            panic!(
                "FAILED - {} error{}!",
                error_count,
                if error_count == 1 { "" } else { "s" }
            );
        }

        println!("\nPASSED");
    }

    #[test]
    fn test_the_spruce_eats_collection() {
        let (fetcher, scraper, extractor) = create_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Jack-O-Lantern",
            link: "https://www.thespruceeats.com/jack-o-lantern-cocktail-recipe-759441",
            image: "https://www.thespruceeats.com/thmb/xnw_a3-0h2feEynmLYOQk064E_o=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/jack-o-lantern-cocktail-recipe-759441-hero-images-1-975cebfd5e294060be0ba8c713529c02.jpg",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Halloween Hypnotist",
            link: "https://www.thespruceeats.com/halloween-hpnotist-recipe-761076",
            image: "https://www.thespruceeats.com/thmb/YdeMWTySSzGOmg4x572UDtgXACE=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/halloween-hpnotist-recipe-761076-hero-01-2e666ba5cbd5439fae40e1cc65bdbabd.jpg",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Mad Eye Martini",
            link: "https://www.thespruceeats.com/mad-eye-martini-recipe-761104",
            image: "https://www.thespruceeats.com/thmb/qsy8Hier0ti6FYEkKy01D83cGak=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/mad-eye-martini-recipe-761104-hero-01-f0975f8d4d284df4b5d3e707e3ed80f5.jpg",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Blood and Sand",
            link: "https://www.thespruceeats.com/blood-and-sand-cocktail-recipe-761336",
            image: "https://www.thespruceeats.com/thmb/b26R1rJ6eOmgom9qRH3hdOtdcc4=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/blood-and-sand-cocktail-recipe-761336-hero-d8e91f5e13d342b5b7a8abe4be6c1f5d.jpg",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Vampire Kiss Martini",
            link: "https://www.thespruceeats.com/vampire-kiss-martini-recipe-761200",
            image: "https://www.thespruceeats.com/thmb/EJ5lfEhz8EqqdwsPJ__tqQmBo74=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/vampire-kiss-martini-recipe-761200-hero-01-062830abd98c470db5e4bc5fe327d3c3.jpg",
        };
        let test_case_section_6 = TestCaseSectionWithImage {
            subtitle: "Fright Night in the Grove",
            link: "https://www.thespruceeats.com/fright-night-in-the-grove-cocktail-760774",
            image: "https://www.thespruceeats.com/thmb/zfmasMziTSTJtHdHarJt2HQqsL4=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/fright-night-in-the-grove-cocktail-760774-hero-01-79c6ebd2ba954db1955b5dab2dce9a8d.jpg",
        };
        let test_case_section_7 = TestCaseSectionWithImage {
            subtitle: "Frog in a Blender",
            link: "https://www.thespruceeats.com/frog-in-a-blender-recipe-761055",
            image: "https://www.thespruceeats.com/thmb/ems_UwBmgL4NDdVBhYX8OFlYzr0=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/frog-in-a-blender-recipe-761055-hero-01-5c54cd20c9e77c0001cff921.jpg",
        };
        let test_case_section_8 = TestCaseSectionWithImage {
            subtitle: "Skeleton Key",
            link: "https://www.thespruceeats.com/skeleton-key-cocktail-recipe-761383",
            image: "https://www.thespruceeats.com/thmb/BRaiGVUEJ8naNTu4FelIM1uohGQ=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/skeleton-key-cocktail-recipe-761383-hero-02-5cdcd93be5d2413f9d3a7a8056964c6e.jpg",
        };
        let test_case_section_9 = TestCaseSectionWithImage {
            subtitle: "Black Widow",
            link: "https://www.thespruceeats.com/black-widow-recipe-761008",
            image: "https://www.thespruceeats.com/thmb/znNmBCCxukTDG4jq-Egb0Cus6rc=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/black-widow-recipe-761008-hero-01-070706a180d04aec9b6736fa1d1f3c19.jpg",
        };
        let test_case_section_10 = TestCaseSectionWithImage {
            subtitle: "Ghostbuster",
            link: "https://www.thespruceeats.com/ghostbuster-cocktail-recipe-759668",
            image: "https://www.thespruceeats.com/thmb/z1k7z8C2iuFIKR5J6JIhOBVeLXE=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/ghostbuster-cocktail-recipe-759668-hero-01-5264544fa57d4d9c8b331c1638e4d8fc.jpg",
        };
        let test_case_section_11 = TestCaseSectionWithImage {
            subtitle: "Zombie",
            link: "https://www.thespruceeats.com/zombie-cocktail-recipe-761643",
            image: "https://www.thespruceeats.com/thmb/oiRY5sLzLk-ytbcZjct6ovpva1g=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/zombie-cocktail-recipe-761643-Hero-5b7424e2c9e77c0050ec7160.jpg",
        };
        let test_case_section_12 = TestCaseSectionWithImage {
            subtitle: "Wolf Bite",
            link: "https://www.thespruceeats.com/wolf-bite-shot-recipe-759565",
            image: "https://www.thespruceeats.com/thmb/tY1ds2xKOxKQa1eYng0yjMRaIxM=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/Wolf-Bite-Shot-56a173203df78cf7726abe54.jpg",
        };
        let test_case_section_13 = TestCaseSectionWithImage {
            subtitle: "Candy Corn Shot",
            link: "https://www.thespruceeats.com/candy-corn-shooter-recipe-759614",
            image: "https://www.thespruceeats.com/thmb/J-pagoch9VRoQBIBDGykpMhc9yw=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/candy-corn-shooter-recipe-759614-hero-cdc381f64705418aa400900c0b79ab47.jpg",
        };
        let test_case = TestCaseWithImage {
            url: "https://www.thespruceeats.com/haunting-halloween-cocktails-759881",
            title: "Spectacular Halloween Cocktails to Spook Your Guests",
            image: "https://www.thespruceeats.com/thmb/RhpEpxyZy5wivA9kH3poaeW6aGY=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/black-widow-recipe-761008-hero-01-5c8801c7c9e77c0001a3e5c9.jpg",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
            ]
            .to_vec(),
        };
        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_cosmopolitan_collection() {
        let (fetcher, scraper, extractor) = create_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Drunken Peanut Butter Cups",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a58358/drunken-peanut-butter-cups-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-cocktails-drunken-peanut-butter-cups-1662511251.jpeg?crop=1xw:0.9993201903467029xh;center,top&resize=980:*",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Apple Cider Doughnut Slushie",
            link: "https://www.delish.com/cooking/recipes/a49600/apple-cider-slushies-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/delish-190920-apple-cider-slushies-0178-landscape-pf-1662511511.jpg?crop=0.669xw:1.00xh;0.196xw,0&resize=980:*",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Zombie Brain Shot",
            link: "https://www.tiktok.com/@thespritzeffect/video/7157066556575403306?_r=1&_t=8eFCmAvZUxm",
            image: "https://hips.hearstapps.com/hmg-prod/images/img-7578-jpg-64c0332feefb2.jpg?crop=0.835xw:1.00xh;0.0748xw,0&resize=980:*",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Hocus Pocus Jello Shots",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a55955/hocus-pocus-jell-o-shots-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/1507330734-delish-hocus-pocus-jello-shots-still001-1662510601.jpg?crop=0.378xw:1.00xh;0.269xw,0&resize=980:*",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Nightmare on Bourbon Street",
            link: "https://www.halfbakedharvest.com/nightmare-on-bourbon-street/",
            image: "https://hips.hearstapps.com/hmg-prod/images/nightmare-on-bourbon-street-1-1662509325.jpg?crop=1xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_6 = TestCaseSectionWithImage {
            subtitle: "Apple Cider Mimosas",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a46963/apple-cider-mimosas-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/0bef95d95637d4f6dfc15d7462098c53-1662510108.jpg?crop=0.8333333333333334xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_7 = TestCaseSectionWithImage {
            subtitle: "Grilled Orange Old-Fashioned",
            link: "https://www.countryliving.com/food-drinks/a40993393/grilled-orange-old-fashioned-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/grilled-orange-old-fashioned-1662509858.jpg?crop=0.646xw:0.780xh;0,0.220xh&resize=980:*",
        };
        let test_case_section_8 = TestCaseSectionWithImage {
            subtitle: "Apple Cinnamon Cider Cups",
            link: "https://www.womansday.com/food-recipes/a33807296/apple-cinnamon-cider-cups-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/apple-cinnamon-cider-cups-1662509051.jpg?crop=0.669xw:1.00xh;0.146xw,0&resize=980:*",
        };
        let test_case_section_9 = TestCaseSectionWithImage {
            subtitle: "Witches' Brew Lemonade",
            link: "https://www.delish.com/holiday-recipes/halloween/a29178988/witches-brew-lemonade-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/witches-brew-lemonade-1662508810.jpg?crop=0.596xw:0.897xh;0,0.0348xh&resize=980:*",
        };
        let test_case_section_10 = TestCaseSectionWithImage {
            subtitle: "Creamsicle Punch",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a52743/creamsicle-punch-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/190409-creamsicle-punch-horizontal-1-1662508571.png?crop=0.665798611111111xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_11 = TestCaseSectionWithImage {
            subtitle: "Poison Apple Cocktail",
            link: "https://www.delish.com/cooking/recipe-ideas/a23878264/poison-apple-cocktails-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/d3e14d682f1e7efa5e832c9cad41dfc5-1662508322.jpg?crop=1.00xw:0.947xh;0,0&resize=980:*",
        };
        let test_case_section_12 = TestCaseSectionWithImage {
            subtitle: "Apricot Bourbon Brew",
            link: "https://www.goodhousekeeping.com/food-recipes/a46066/apricot-bourbon-brew-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-party-potion-punch-1662507902.jpg?crop=1xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_13 = TestCaseSectionWithImage {
            subtitle: "Tart Cherry Eyeball Punch",
            link: "https://www.countryliving.com/food-drinks/a36687070/tart-cherry-eyeball-punch/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-party-cocktails-1016-1662504161.jpg?crop=0.835xw:1.00xh;0.0260xw,0&resize=980:*",
        };
        let test_case_section_14 = TestCaseSectionWithImage {
            subtitle: "Cider Sidecar",
            link: "https://www.countryliving.com/food-drinks/a23326064/cider-sidecar-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/cider-sidecar-cl-1018-1662503635.jpg?crop=1xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_15 = TestCaseSectionWithImage {
            subtitle: "Sleepy Hollow Cocktail",
            link: "https://www.halfbakedharvest.com/sleepy-hollow-cocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/sleepy-hollow-cocktail-1-1662503073.jpg?crop=1xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_16 = TestCaseSectionWithImage {
            subtitle: "Cinnamon Apple Margarita",
            link: "https://lalospirits.com/",
            image: "https://hips.hearstapps.com/hmg-prod/images/lalo-cinnamon-apple-marg-2-64c02202db827.jpeg?crop=0.650xw:0.650xh;0.350xw,0.241xh&resize=980:*",
        };
        let test_case_section_17 = TestCaseSectionWithImage {
            subtitle: "Blood Orange Sangria",
            link: "https://www.howsweeteats.com/2013/02/blood-orange-sangria/",
            image: "https://hips.hearstapps.com/hmg-prod/images/2022-09-06-2-1662502497.png?crop=0.321xw:0.722xh;0.184xw,0.114xh&resize=980:*",
        };
        let test_case_section_18 = TestCaseSectionWithImage {
            subtitle: "Black Widow Smash",
            link: "https://www.halfbakedharvest.com/the-black-widow-smash/",
            image: "https://hips.hearstapps.com/hmg-prod/images/2022-09-06-1662501529.png?crop=0.231xw:0.517xh;0.671xw,0.239xh&resize=980:*",
        };
        let test_case_section_19 = TestCaseSectionWithImage {
            subtitle: "Hocus Pocus Punch",
            link: "https://www.howsweeteats.com/2019/10/hocus-pocus-punch-p-s-its-a-mocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/hocus-pocus-punch-3-1662501062.jpg?crop=0.8907892392659897xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_20 = TestCaseSectionWithImage {
            subtitle: "Blood Moon Cocktail",
            link: "https://www.thesexton.com/cocktails/the-sexton-blood-moon/",
            image: "https://www.thesexton.com/wp-content/uploads/2021/11/sexton-bloodmoon.jpg",
        };
        let test_case_section_21 = TestCaseSectionWithImage {
            subtitle: "Pumpkin Cider",
            link: "https://www.newamsterdamvodka.com/",
            image: "https://hips.hearstapps.com/hmg-prod/images/new-amsterdam-pumpkin-cider-1657553140.jpeg?crop=0.8263695450324977xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_22 = TestCaseSectionWithImage {
            subtitle: "The Vampire’s Kiss Cocktail",
            link: "https://www.halfbakedharvest.com/the-vampires-kiss-cocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/vampires-kiss-1628524783.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_23 = TestCaseSectionWithImage {
            subtitle: "Blood Rising Cocktail",
            link: "https://www.halfbakedharvest.com/blood-rising-cocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/blood-rising-1628524830.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_24 = TestCaseSectionWithImage {
            subtitle: "Pumpkin Punch",
            link: "https://www.halfbakedharvest.com/pumpkin-punch/",
            image: "https://hips.hearstapps.com/hmg-prod/images/pumpkin-punch-1628524880.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_25 = TestCaseSectionWithImage {
            subtitle: "Ghost in the Orchard Cocktail",
            link: "https://www.halfbakedharvest.com/ghost-in-the-orchard-cocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/ghost-in-the-orchard-1628524929.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_26 = TestCaseSectionWithImage {
            subtitle: "Mummy White Russian",
            link: "https://www.halfbakedharvest.com/mummy-white-russian/",
            image: "https://hips.hearstapps.com/hmg-prod/images/mummy-white-russian-1628524977.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_27 = TestCaseSectionWithImage {
            subtitle: "Mystic Moon Cocktail",
            link: "https://www.halfbakedharvest.com/mystic-moon-cocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/mystic-moon-1628525021.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_28 = TestCaseSectionWithImage {
            subtitle: "Bourbon Butterbeer",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a56104/spellbound-cocktail-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/spellbound-cocktail-2-1628525073.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_29 = TestCaseSectionWithImage {
            subtitle: "Spellbound Cocktail",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a46964/pomegranate-cider-mimosas-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/pomegranate-cider-mimosas-3-1628525128.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_30 = TestCaseSectionWithImage {
            subtitle: "Pomegranate Cider Mimosas",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a54858/spiked-jolly-rancher-punch-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/spiked-jolly-rancher-punch-5-1628525172.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_31 = TestCaseSectionWithImage {
            subtitle: "Spiked Jolly Rancher Punch",
            link: "https://www.delish.com/cooking/recipe-ideas/a26216721/hot-buttered-rum-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/hot-buttered-rum-2-1628525217.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_32 = TestCaseSectionWithImage {
            subtitle: "Blood Wolf Moon",
            link: "https://www.delish.com/cooking/recipe-ideas/a26216721/hot-buttered-rum-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/hot-buttered-rum-2-1628525217.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_33 = TestCaseSectionWithImage {
            subtitle: "Haunted Graveyard",
            link: "https://www.sprinklesandsprouts.com/haunted-graveyard-a-halloween-cocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/haunted-graveyard-halloween-cocktail-2-1628525313.jpeg?crop=1xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_34 = TestCaseSectionWithImage {
            subtitle: "Pumpkin Spice White Russian",
            link: "https://www.thecookierookie.com/pumpkin-spice-white-russian-cocktail/",
            image: "https://www.thecookierookie.com/wp-content/uploads/2018/09/pumpkin-spice-white-russian-5-of-13.jpg",
        };
        let test_case_section_35 = TestCaseSectionWithImage {
            subtitle: "Monster Mash Margaritas",
            link: "https://www.freutcake.com/in-the-kitchen/drinks-anyone/monster-mash-margaritas/",
            image: "https://hips.hearstapps.com/hmg-prod/images/monster-mash-cocktail-3-1628525458.jpeg?crop=1.00xw:1.00xh;0.00170xw,0&resize=980:*",
        };
        let test_case_section_36 = TestCaseSectionWithImage {
            subtitle: "Cacao Imperial Old-Fashioned Cocktail",
            link: "https://ronbarcelo.com/en/rum/imperial/",
            image: "https://hips.hearstapps.com/hmg-prod/images/ronbarcelocacaoimperialoldfashionedcocktailtiny-1628525657.png?crop=0.451xw:1.00xh;0.523xw,0&resize=980:*",
        };
        let test_case_section_37 = TestCaseSectionWithImage {
            subtitle: "Blood and Sand Cocktail With Lychee Eyeball",
            link: "https://go.redirectingat.com?id=74968X1525071&url=https%3A%2F%2Fwww.hellofresh.com%2F",
            image: "https://hips.hearstapps.com/hmg-prod/images/hf160928-extrashot-us-halloweentipsheet-42-low-1050x1575-1628527296.jpeg?crop=1xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_38 = TestCaseSectionWithImage {
            subtitle: "Fright White Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_39 = TestCaseSectionWithImage {
            subtitle: "The Apparition Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_40 = TestCaseSectionWithImage {
            subtitle: "Dark and Stormy Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_41 = TestCaseSectionWithImage {
            subtitle: "Absolut Masquerade Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_42 = TestCaseSectionWithImage {
            subtitle: "The Gravedigger Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_43 = TestCaseSectionWithImage {
            subtitle: "Sugarsnake Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_44 = TestCaseSectionWithImage {
            subtitle: "Black Cauldron Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_45 = TestCaseSectionWithImage {
            subtitle: "The Boneyard Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_46 = TestCaseSectionWithImage {
            subtitle: "Heat of the Moment Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_47 = TestCaseSectionWithImage {
            subtitle: "Smoked Pumpkin Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_48 = TestCaseSectionWithImage {
            subtitle: "Midnight’s Shadow Cocktail",
            link: "",
            image: "",
        };
        let test_case = TestCaseWithImage {
            url: "https://www.cosmopolitan.com/food-cocktails/a4896/spooky-halloween-cocktails/",
            title: "48 Spooky Halloween Cocktails to Mix Up for Ghouls Night",
            image: "https://hips.hearstapps.com/hmg-prod/images/48-spooky-halloween-cocktails-to-mix-up-for-ghouls-night-6508b1c4451d9.png?crop=1xw:0.9944392956441149xh;center,top&resize=1200:*",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
                test_case_section_14,
                test_case_section_15,
                test_case_section_16,
                test_case_section_17,
                test_case_section_18,
                test_case_section_19,
                test_case_section_20,
                test_case_section_21,
                test_case_section_22,
                test_case_section_23,
                test_case_section_24,
                test_case_section_25,
                test_case_section_26,
                test_case_section_27,
                test_case_section_28,
                test_case_section_29,
                test_case_section_30,
                test_case_section_31,
                test_case_section_32,
                test_case_section_33,
                test_case_section_34,
                test_case_section_35,
                test_case_section_36,
                test_case_section_37,
                test_case_section_38,
                test_case_section_39,
                test_case_section_40,
                test_case_section_41,
                test_case_section_42,
                test_case_section_43,
                test_case_section_44,
                test_case_section_45,
                test_case_section_46,
                test_case_section_47,
                test_case_section_48,
            ]
            .to_vec(),
        };
        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_delish_jsonld_metadata_presence() {
        // Test to check if expected links are in JSON-LD or metadata
        let url = "https://www.delish.com/holiday-recipes/halloween/g2471/halloween-drink-recipes/";

        // Fetch the HTML
        let html = std::process::Command::new("curl")
            .arg("-s")
            .arg("--max-time")
            .arg("10")
            .arg(url)
            .output()
            .expect("Failed to fetch HTML")
            .stdout;

        let html_str = String::from_utf8_lossy(&html);

        // All 50 expected article IDs from test_delish_collection
        let expected_ids = vec![
            ("Witches' Brew Lemonade", "a29178988"),
            ("Spicy Apple Cider Margaritas", "a62335889"),
            ("Halloween Sangria", "a44787887"),
            ("Gummy Bear Cocktail", "a60336437"),
            ("Black Magic Margaritas", "a55953"),
            ("Pumpkin Margarita", "a61915548"),
            ("Black Widow Cocktail", "a62452802"),
            ("Sparkling Apple Cider Sangria", "a22877349"),
            ("Color-Changing Margaritas", "a27183454"),
            ("Pumpkin Juice", "a41447206"),
            ("Blood Orange Mocktail Spritzer", "a46298758"),
            ("Pumpkin Spice White Russians", "a56718"),
            ("Green Apple Moscato Sangria", "a28691436"),
            ("Brain Big Batch Jell-O Shot", "a49582"),
            ("Apple Cider Margaritas", "a55800"),
            ("Espresso Mocha Crunch Mocktail", "a46148213"),
            ("Poison Apple Cocktails", "a23878264"),
            ("Campfire Mules", "a44601290"),
            ("Bloody Mary Syringes", "a24132876"),
            ("Apple Pie Bourbon Shots", "a43810"),
            ("Apple Cider Spritz", "a41312899"),
            ("Spiked Hot Chocolate", "a42277098"),
            ("Boozy Screamsicle Shakes", "a29007809"),
            ("Boozy Butterbeer Punch", "a55804"),
            ("Cranberry Aperol Spritz", "a45293549"),
            ("Sour Patch Jell-O Shots", "a63959584"),
            ("Drunken Peanut Butter Cups", "a58358"),
            ("Frankenpunch", "a44172"),
            ("Candy Corn Jell-O Shots", "a49781"),
            ("Jekyll & Gin", "a44311"),
            ("Hocus Pocus Jell-O Shots", "a55955"),
            ("Apple Cider Slushies", "a49600"),
            ("Pumpkin Pie Punch", "a44183"),
            ("Espresso Martini", "a36356671"),
            ("Washington Apple Shot", "a40515769"),
            ("Harvest Punch", "a55182"),
            ("Sweet Poison Cocktail", "a43892"),
            ("Apple Cider Sangria", "a43666"),
            ("Apple Cider Mimosas", "a46963"),
            ("Black Magic Jell-O Shots", "a23876979"),
            ("Good & Evil Cocktail", "a43895"),
            ("The Zombie Cocktail", "a43896"),
            ("Big Apple Manhattan", "a30123165"),
            ("The Risen From The Grave Cocktail", "a43890"),
            ("Bourbon Milk Punch", "a52301"),
            ("Transformation Cocktail", "a43893"),
            ("Drunken Pumpkin Latte", "a33865346"),
            ("J-E-L-L-Glow Shots", "a44347"),
            ("Cotton Candy Shots", "a44306"),
            ("Pumpkin Pie Sangria", "a56371"),
        ];

        println!("\n=== Checking for links in JSON-LD and metadata ===\n");

        // Extract all JSON-LD scripts
        let json_ld_pattern =
            regex::Regex::new(r#"<script[^>]*type="application/ld\+json"[^>]*>(.*?)</script>"#)
                .unwrap();
        let mut json_ld_count = 0;
        let mut total_json_ld_blocks = 0;

        for cap in json_ld_pattern.captures_iter(&html_str) {
            total_json_ld_blocks += 1;
            if let Some(json_content) = cap.get(1) {
                let json_str = json_content.as_str();
                println!(
                    "Found JSON-LD block #{} with {} bytes",
                    total_json_ld_blocks,
                    json_str.len()
                );

                // Check if it contains ItemList
                if json_str.contains("ItemList") {
                    println!("  └─ Contains ItemList!");
                }

                // Count how many of our expected IDs are in this JSON-LD
                let mut found_in_block = 0;
                for (name, id) in &expected_ids {
                    if json_str.contains(id) {
                        found_in_block += 1;
                        json_ld_count += 1;
                        println!("    ✅ Found in JSON-LD: {} - {}", name, id);
                    }
                }
                if found_in_block > 0 {
                    println!("  └─ Contains {} expected article IDs", found_in_block);
                }
            }
        }

        println!("\n=== Checking meta tags ===");

        // Check meta tags for article IDs
        let meta_pattern = regex::Regex::new(r#"<meta[^>]*content="([^"]*)"[^>]*>"#).unwrap();
        let mut meta_count = 0;

        for cap in meta_pattern.captures_iter(&html_str) {
            if let Some(content) = cap.get(1) {
                let content_str = content.as_str();
                for (name, id) in &expected_ids {
                    if content_str.contains(id) {
                        meta_count += 1;
                        println!("✅ Found in meta tag: {} - {}", name, id);
                    }
                }
            }
        }

        println!("\n=== Checking data attributes ===");

        // Check data attributes
        let data_pattern = regex::Regex::new(r#"data-[^=]*="([^"]*)"#).unwrap();
        let mut data_count = 0;

        for cap in data_pattern.captures_iter(&html_str) {
            if let Some(content) = cap.get(1) {
                let content_str = content.as_str();
                for (name, id) in &expected_ids {
                    if content_str.contains(id) {
                        data_count += 1;
                        println!("✅ Found in data attribute: {} - {}", name, id);
                        break; // Only count once per attribute
                    }
                }
            }
        }

        println!("\n=== Summary ===");
        println!("Total JSON-LD blocks found: {}", total_json_ld_blocks);
        println!(
            "Article IDs found in JSON-LD: {}/50",
            json_ld_count / expected_ids.len()
        );
        println!(
            "Article IDs found in meta tags: {}/50",
            meta_count / expected_ids.len()
        );
        println!(
            "Article IDs found in data attributes: {}/50",
            data_count / expected_ids.len()
        );

        // Check for unique IDs across all sources
        let mut all_found = std::collections::HashSet::new();
        for (_name, id) in &expected_ids {
            if html_str.contains(id) {
                all_found.insert(id.to_string());
            }
        }

        println!(
            "\nTotal unique article IDs found anywhere in HTML: {}/50",
            all_found.len()
        );

        // List missing IDs
        let missing: Vec<_> = expected_ids
            .iter()
            .filter(|(_, id)| !all_found.contains(*id))
            .collect();

        if !missing.is_empty() {
            println!("\n❌ Completely missing from HTML:");
            for (name, id) in missing {
                println!("  - {}: {}", name, id);
            }
        }
    }

    #[test]
    fn test_delish_full_url_html_presence() {
        // Test to check which expected Delish links are actually in the HTML
        let url = "https://www.delish.com/holiday-recipes/halloween/g2471/halloween-drink-recipes/";
        // Fetch the HTML
        let html = std::process::Command::new("curl")
            .arg("-s")
            .arg("--max-time")
            .arg("10")
            .arg(url)
            .output()
            .expect("Failed to fetch HTML")
            .stdout;
        let html_str = String::from_utf8_lossy(&html);
        // All 50 expected links from test_delish_collection
        let expected_links = vec![
            ("Witches' Brew Lemonade", "https://www.delish.com/holiday-recipes/halloween/a29178988/witches-brew-lemonade-recipe/"),
            ("Spicy Apple Cider Margaritas", "https://www.delish.com/cooking/recipe-ideas/a62335889/spicy-apple-cider-margarita-recipe/"),
            ("Halloween Sangria", "https://www.delish.com/cooking/recipe-ideas/a44787887/halloween-sangria-recipe/"),
            ("Gummy Bear Cocktail", "https://www.delish.com/cooking/recipe-ideas/a60336437/gummy-bears-cocktail-recipe/"),
            ("Black Magic Margaritas", "https://www.delish.com/cooking/recipe-ideas/a55953/black-magic-margaritas-recipe/"),
            ("Pumpkin Margarita", "https://www.delish.com/cooking/recipe-ideas/a61915548/pumpkin-margarita-recipe/"),
            ("Black Widow Cocktail", "https://www.delish.com/cooking/recipe-ideas/a62452802/black-widow-cocktail-recipe/"),
            ("Sparkling Apple Cider Sangria", "https://www.delish.com/cooking/recipe-ideas/a22877349/sparkling-apple-cider-sangria-recipe/"),
            ("Color-Changing Margaritas", "https://www.delish.com/cooking/recipe-ideas/a27183454/color-changing-margaritas-recipe/"),
            ("Pumpkin Juice", "https://www.delish.com/cooking/a41447206/harry-potter-pumpkin-juice-recipe/"),
            ("Blood Orange Mocktail Spritzer", "https://www.delish.com/cooking/recipe-ideas/a46298758/blood-orange-mocktail-spritzer-recipe/"),
            ("Pumpkin Spice White Russians", "https://www.delish.com/cooking/recipe-ideas/recipes/a56718/pumpkin-spice-white-russians/"),
            ("Green Apple Moscato Sangria", "https://www.delish.com/cooking/recipe-ideas/a28691436/green-apple-moscato-sangria-recipe/"),
            ("Brain Big Batch Jell-O Shot", "https://www.delish.com/cooking/recipe-ideas/recipes/a49582/brain-big-batch-jell-o-shot-recipe/"),
            ("Apple Cider Margaritas", "https://www.delish.com/cooking/recipe-ideas/recipes/a55800/apple-cider-margaritas-recipe/"),
            ("Espresso Mocha Crunch Mocktail", "https://www.delish.com/cooking/recipe-ideas/a46148213/espresso-mocha-crunch-mocktail-recipe/"),
            ("Poison Apple Cocktails", "https://www.delish.com/cooking/recipe-ideas/a23878264/poison-apple-cocktails-recipe/"),
            ("Campfire Mules", "https://www.delish.com/cooking/recipe-ideas/a44601290/campfire-mules-cocktail-recipe/"),
            ("Bloody Mary Syringes", "https://www.delish.com/cooking/recipe-ideas/a24132876/bloody-mary-syringes-recipe/"),
            ("Apple Pie Bourbon Shots", "https://www.delish.com/cooking/recipe-ideas/recipes/a43810/apple-pie-bourbon-shots-recipe/"),
            ("Apple Cider Spritz", "https://www.delish.com/cooking/recipe-ideas/a41312899/apple-cider-spritz-recipe/"),
            ("Spiked Hot Chocolate", "https://www.delish.com/cooking/recipe-ideas/a42277098/spiked-hot-chocolate-recipe/"),
            ("Boozy Screamsicle Shakes", "https://www.delish.com/holiday-recipes/halloween/a29007809/boozy-screamsicle-shakes-recipe/"),
            ("Boozy Butterbeer Punch", "https://www.delish.com/cooking/recipe-ideas/recipes/a55804/boozy-butterbeer-punch-recipe/"),
            ("Cranberry Aperol Spritz", "https://www.delish.com/cooking/recipe-ideas/a45293549/cranberry-aperol-spritz-recipe/"),
            ("Sour Patch Jell-O Shots", "https://www.delish.com/cooking/recipe-ideas/a63959584/sour-patch-jello-shots-recipe/"),
            ("Drunken Peanut Butter Cups", "https://www.delish.com/cooking/recipe-ideas/recipes/a58358/drunken-peanut-butter-cups-recipe/"),
            ("Frankenpunch", "https://www.delish.com/cooking/recipe-ideas/recipes/a44172/frankenpunch-lime-sherbert-recipe/"),
            ("Candy Corn Jell-O Shots", "https://www.delish.com/holiday-recipes/halloween/recipes/a49781/easy-candy-corn-jello-shots-recipe/"),
            ("Jekyll & Gin", "https://www.delish.com/cooking/recipe-ideas/recipes/a44311/jekyll-gin-glowing-cocktails-glow-party-ideas/"),
            ("Hocus Pocus Jell-O Shots", "https://www.delish.com/cooking/recipe-ideas/recipes/a55955/hocus-pocus-jell-o-shots-recipe/"),
            ("Apple Cider Slushies", "https://www.delish.com/cooking/recipes/a49600/apple-cider-slushies-recipe/"),
            ("Pumpkin Pie Punch", "https://www.delish.com/cooking/recipe-ideas/recipes/a44183/spiked-pumpkin-pie-punch-recipe/"),
            ("Espresso Martini", "https://www.delish.com/cooking/a36356671/espresso-martini/"),
            ("Washington Apple Shot", "https://www.delish.com/cooking/recipe-ideas/a40515769/washington-apple-shot-recipe/"),
            ("Harvest Punch", "https://www.delish.com/cooking/recipe-ideas/a55182/cider-harvest-punch-recipe/"),
            ("Sweet Poison Cocktail", "https://www.delish.com/cooking/recipe-ideas/recipes/a43892/sweet-poison-cocktail/"),
            ("Apple Cider Sangria", "https://www.delish.com/cooking/recipe-ideas/recipes/a43666/apple-cider-sangria-recipe/"),
            ("Apple Cider Mimosas", "https://www.delish.com/cooking/recipe-ideas/recipes/a46963/apple-cider-mimosas-recipe/"),
            ("Black Magic Jell-O Shots", "https://www.delish.com/cooking/recipe-ideas/a23876979/black-magic-jell-o-shots-recipe/"),
            ("Good & Evil Cocktail", "https://www.delish.com/cooking/recipe-ideas/recipes/a43895/halloween-cocktail-ideas-good-and-evil-cocktail-recipe/"),
            ("The Zombie Cocktail", "https://www.delish.com/cooking/recipe-ideas/recipes/a43896/halloween-cocktail-ideas-zombie-cocktail-recipe/"),
            ("Big Apple Manhattan", "https://www.delish.com/cooking/recipe-ideas/a30123165/big-apple-manhattan-recipe/"),
            ("The Risen From The Grave Cocktail", "https://www.delish.com/cooking/recipe-ideas/recipes/a43890/the-risen-from-the-grave-cocktail-recipe/"),
            ("Bourbon Milk Punch", "https://www.delish.com/cooking/recipe-ideas/recipes/a52301/bourbon-milk-punch-recipe/"),
            ("Transformation Cocktail", "https://www.delish.com/cooking/recipe-ideas/recipes/a43893/halloween-cocktail-ideas-transformation-cocktail-recipe/"),
            ("Drunken Pumpkin Latte", "https://www.delish.com/cooking/recipe-ideas/a33865346/drunken-pumpkin-latte-recipe/"),
            ("J-E-L-L-Glow Shots", "https://www.delish.com/cooking/recipe-ideas/recipes/a44347/glowing-jell-o-shots-glow-party-foods/"),
            ("Cotton Candy Shots", "https://www.delish.com/cooking/recipe-ideas/recipes/a44306/cotton-candy-shots-recipe/"),
            ("Pumpkin Pie Sangria", "https://www.delish.com/cooking/recipe-ideas/recipes/a56371/pumpkin-pie-sangria-recipe/"),
        ];

        println!("\n=== Checking which Delish recipe links are in the HTML ===\n");

        let mut found_count = 0;
        let mut missing_count = 0;

        for (name, expected_url) in &expected_links {
            // Extract just the unique part of the URL for searching
            let url_parts: Vec<&str> = expected_url.split('/').collect();
            let unique_part = url_parts.get(url_parts.len() - 2).unwrap_or(&"");

            if html_str.contains(unique_part) {
                println!("✅ FOUND: {} - {}", name, unique_part);
                found_count += 1;

                // Also check if the full URL is there
                if html_str.contains(expected_url) {
                    println!("   └─ Full URL present");
                } else {
                    println!("   └─ Only partial URL present");
                }
            } else {
                println!("❌ MISSING: {} - {}", name, unique_part);
                missing_count += 1;
            }
        }

        println!("\n=== Summary ===");
        println!("Found: {}/{}", found_count, expected_links.len());
        println!("Missing: {}/{}", missing_count, expected_links.len());

        // Fail the test if not all links are found
        if missing_count > 0 {
            panic!(
                "Not all expected links found in HTML! Missing {}/{} links:\n{}",
                missing_count,
                expected_links.len(),
                expected_links
                    .iter()
                    .filter_map(|(name, url)| {
                        let url_parts: Vec<&str> = url.split('/').collect();
                        let unique_part = url_parts.get(url_parts.len() - 2)?;
                        if !html_str.contains(unique_part) {
                            Some(format!("  - {}: {}", name, unique_part))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }

        // Also check what recipe links ARE present
        println!("\n=== Sample of recipe links that ARE in the HTML ===");
        let recipe_pattern = regex::Regex::new(r#"href="([^"]*/(recipe|recipes)/[^"]*)"#).unwrap();
        let mut recipe_urls = std::collections::HashSet::new();

        for cap in recipe_pattern.captures_iter(&html_str) {
            if let Some(url) = cap.get(1) {
                let url_str = url.as_str();
                if url_str.contains("delish.com") && !url_str.contains("#") {
                    recipe_urls.insert(url_str);
                }
            }
        }

        let mut urls_vec: Vec<_> = recipe_urls.iter().collect();
        urls_vec.sort();

        for url in urls_vec {
            println!("  - {}", url);
        }

        println!("\nTotal recipe links found: {}", recipe_urls.len());
    }

    #[test]
    fn test_delish_collection() {
        let (fetcher, scraper, extractor) = create_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSection {
            subtitle: "Witches' Brew Lemonade",
            link: "https://www.delish.com/holiday-recipes/halloween/a29178988/witches-brew-lemonade-recipe/",
        };
        let test_case_section_2 = TestCaseSection {
            subtitle: "Spicy Apple Cider Margaritas",
            link: "https://www.delish.com/cooking/recipe-ideas/a62335889/spicy-apple-cider-margarita-recipe/",
        };
        let test_case_section_3 = TestCaseSection {
            subtitle: "Halloween Sangria",
            link: "https://www.delish.com/cooking/recipe-ideas/a44787887/halloween-sangria-recipe/",
        };
        let test_case_section_4 = TestCaseSection {
            subtitle: "Gummy Bear Cocktail",
            link:
                "https://www.delish.com/cooking/recipe-ideas/a60336437/gummy-bears-cocktail-recipe/",
        };
        let test_case_section_5 = TestCaseSection {
            subtitle: "Black Magic Margaritas",
            link:
                "https://www.delish.com/cooking/recipe-ideas/a55953/black-magic-margaritas-recipe/",
        };
        let test_case_section_6 = TestCaseSection {
            subtitle: "Pumpkin Margarita",
            link: "https://www.delish.com/cooking/recipe-ideas/a61915548/pumpkin-margarita-recipe/",
        };
        let test_case_section_7 = TestCaseSection {
            subtitle: "Black Widow Cocktail",
            link:
                "https://www.delish.com/cooking/recipe-ideas/a62452802/black-widow-cocktail-recipe/",
        };
        let test_case_section_8 = TestCaseSection {
            subtitle: "Sparkling Apple Cider Sangria",
            link: "https://www.delish.com/cooking/recipe-ideas/a22877349/sparkling-apple-cider-sangria-recipe/",
        };
        let test_case_section_9 = TestCaseSection {
            subtitle: "Color-Changing Margaritas",
            link: "https://www.delish.com/cooking/recipe-ideas/a27183454/color-changing-margaritas-recipe/",
        };
        let test_case_section_10 = TestCaseSection {
            subtitle: "Pumpkin Juice",
            link: "https://www.delish.com/cooking/a41447206/harry-potter-pumpkin-juice-recipe/",
        };
        let test_case_section_11 = TestCaseSection {
            subtitle: "Blood Orange Mocktail Spritzer",
            link: "https://www.delish.com/cooking/recipe-ideas/a46298758/blood-orange-mocktail-spritzer-recipe/",
        };
        let test_case_section_12 = TestCaseSection {
            subtitle: "Pumpkin Spice White Russians",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a56718/pumpkin-spice-white-russians/",
        };
        let test_case_section_13 = TestCaseSection {
            subtitle: "Green Apple Moscato Sangria",
            link: "https://www.delish.com/cooking/recipe-ideas/a28691436/green-apple-moscato-sangria-recipe/",
        };
        let test_case_section_14 = TestCaseSection {
            subtitle: "Brain Big Batch Jell-O Shot",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a49582/brain-big-batch-jell-o-shot-recipe/",
        };
        let test_case_section_15 = TestCaseSection {
            subtitle: "Apple Cider Margaritas",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a55800/apple-cider-margaritas-recipe/",
        };
        let test_case_section_16 = TestCaseSection {
            subtitle: "Espresso Mocha Crunch Mocktail",
            link: "https://www.delish.com/cooking/recipe-ideas/a46148213/espresso-mocha-crunch-mocktail-recipe/",
        };
        let test_case_section_17 = TestCaseSection {
            subtitle: "Poison Apple Cocktails",
            link: "https://www.delish.com/cooking/recipe-ideas/a23878264/poison-apple-cocktails-recipe/",
        };
        let test_case_section_18 = TestCaseSection {
            subtitle: "Campfire Mules",
            link: "https://www.delish.com/cooking/recipe-ideas/a44601290/campfire-mules-cocktail-recipe/",
        };
        let test_case_section_19 = TestCaseSection {
            subtitle: "Bloody Mary Syringes",
            link:
                "https://www.delish.com/cooking/recipe-ideas/a24132876/bloody-mary-syringes-recipe/",
        };
        let test_case_section_20 = TestCaseSection {
            subtitle: "Apple Pie Bourbon Shots",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a43810/apple-pie-bourbon-shots-recipe/",
        };
        let test_case_section_21 = TestCaseSection {
            subtitle: "Apple Cider Spritz",
            link:
                "https://www.delish.com/cooking/recipe-ideas/a41312899/apple-cider-spritz-recipe/",
        };
        let test_case_section_22 = TestCaseSection {
            subtitle: "Spiked Hot Chocolate",
            link:
                "https://www.delish.com/cooking/recipe-ideas/a42277098/spiked-hot-chocolate-recipe/",
        };
        let test_case_section_23 = TestCaseSection {
            subtitle: "Boozy Screamsicle Shakes",
            link: "https://www.delish.com/holiday-recipes/halloween/a29007809/boozy-screamsicle-shakes-recipe/",
        };
        let test_case_section_24 = TestCaseSection {
            subtitle: "Boozy Butterbeer Punch",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a55804/boozy-butterbeer-punch-recipe/",
        };
        let test_case_section_25 = TestCaseSection {
            subtitle: "Cranberry Aperol Spritz",
            link: "https://www.delish.com/cooking/recipe-ideas/a45293549/cranberry-aperol-spritz-recipe/",
        };
        let test_case_section_26 = TestCaseSection {
            subtitle: "Sour Patch Jell-O Shots",
            link: "https://www.delish.com/cooking/recipe-ideas/a63959584/sour-patch-jello-shots-recipe/",
        };
        let test_case_section_27 = TestCaseSection {
            subtitle: "Drunken Peanut Butter Cups",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a58358/drunken-peanut-butter-cups-recipe/",
        };
        let test_case_section_28 = TestCaseSection {
            subtitle: "Frankenpunch",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a44172/frankenpunch-lime-sherbert-recipe/",
        };
        let test_case_section_29 = TestCaseSection {
            subtitle: "Candy Corn Jell-O Shots",
            link: "https://www.delish.com/holiday-recipes/halloween/recipes/a49781/easy-candy-corn-jello-shots-recipe/",
        };
        let test_case_section_30 = TestCaseSection {
            subtitle: "Jekyll & Gin",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a44311/jekyll-gin-glowing-cocktails-glow-party-ideas/",
        };
        let test_case_section_31 = TestCaseSection {
            subtitle: "Hocus Pocus Jell-O Shots",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a55955/hocus-pocus-jell-o-shots-recipe/",
        };
        let test_case_section_32 = TestCaseSection {
            subtitle: "Apple Cider Slushies",
            link: "https://www.delish.com/cooking/recipes/a49600/apple-cider-slushies-recipe/",
        };
        let test_case_section_33 = TestCaseSection {
            subtitle: "Pumpkin Pie Punch",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a44183/spiked-pumpkin-pie-punch-recipe/",
        };
        let test_case_section_34 = TestCaseSection {
            subtitle: "Espresso Martini",
            link: "https://www.delish.com/cooking/a36356671/espresso-martini/",
        };
        let test_case_section_35 = TestCaseSection {
            subtitle: "Washington Apple Shot",
            link: "https://www.delish.com/cooking/recipe-ideas/a40515769/washington-apple-shot-recipe/",
        };
        let test_case_section_36 = TestCaseSection {
            subtitle: "Harvest Punch",
            link: "https://www.delish.com/cooking/recipe-ideas/a55182/cider-harvest-punch-recipe/",
        };
        let test_case_section_37 = TestCaseSection {
            subtitle: "Sweet Poison Cocktail",
            link:
                "https://www.delish.com/cooking/recipe-ideas/recipes/a43892/sweet-poison-cocktail/",
        };
        let test_case_section_38 = TestCaseSection {
            subtitle: "Apple Cider Sangria",
            link: "https://www.delish.com/cooking/recipe-ideas/a46963/apple-cider-mimosas-recipe/",
        };
        let test_case_section_39 = TestCaseSection {
            subtitle: "Apple Cider Mimosas",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a46963/apple-cider-mimosas-recipe/",
        };
        let test_case_section_40 = TestCaseSection {
            subtitle: "Black Magic Jell-O Shots",
            link: "https://www.delish.com/cooking/recipe-ideas/a23876979/black-magic-jell-o-shots-recipe/",
        };
        let test_case_section_41 = TestCaseSection {
            subtitle: "Good & Evil Cocktail",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a43895/halloween-cocktail-ideas-good-and-evil-cocktail-recipe/",
        };
        let test_case_section_42 = TestCaseSection {
            subtitle: "The Zombie Cocktail",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a43896/halloween-cocktail-ideas-zombie-cocktail-recipe/",
        };
        let test_case_section_43 = TestCaseSection {
            subtitle: "Big Apple Manhattan",
            link:
                "https://www.delish.com/cooking/recipe-ideas/a30123165/big-apple-manhattan-recipe/",
        };
        let test_case_section_44 = TestCaseSection {
            subtitle: "The Risen From The Grave Cocktail",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a43890/the-risen-from-the-grave-cocktail-recipe/",
        };
        let test_case_section_45 = TestCaseSection {
            subtitle: "Bourbon Milk Punch",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a52301/bourbon-milk-punch-recipe/",
        };
        let test_case_section_46 = TestCaseSection {
            subtitle: "Transformation Cocktail",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a43893/halloween-cocktail-ideas-transformation-cocktail-recipe/",
        };
        let test_case_section_47 = TestCaseSection {
            subtitle: "Drunken Pumpkin Latte",
            link: "https://www.delish.com/cooking/recipe-ideas/a33865346/drunken-pumpkin-latte-recipe/",
        };
        let test_case_section_48 = TestCaseSection {
            subtitle: "J-E-L-L-Glow Shots",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a44347/glowing-jell-o-shots-glow-party-foods/",
        };
        let test_case_section_49 = TestCaseSection {
            subtitle: "Cotton Candy Shots",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a44306/cotton-candy-shots-recipe/",
        };
        let test_case_section_50 = TestCaseSection {
            subtitle: "Pumpkin Pie Sangria",
            link: "https://www.delish.com/holiday-recipes/halloween/g2471/halloween-drink-recipes/#slide-50",
        };
        let test_case = TestCase {
            url: "https://www.delish.com/holiday-recipes/halloween/g2471/halloween-drink-recipes/",
            title: "Double-Double, These 50 Halloween Cocktails Are Trouble",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
                test_case_section_14,
                test_case_section_15,
                test_case_section_16,
                test_case_section_17,
                test_case_section_18,
                test_case_section_19,
                test_case_section_20,
                test_case_section_21,
                test_case_section_22,
                test_case_section_23,
                test_case_section_24,
                test_case_section_25,
                test_case_section_26,
                test_case_section_27,
                test_case_section_28,
                test_case_section_29,
                test_case_section_30,
                test_case_section_31,
                test_case_section_32,
                test_case_section_33,
                test_case_section_34,
                test_case_section_35,
                test_case_section_36,
                test_case_section_37,
                test_case_section_38,
                test_case_section_39,
                test_case_section_40,
                test_case_section_41,
                test_case_section_42,
                test_case_section_43,
                test_case_section_44,
                test_case_section_45,
                test_case_section_46,
                test_case_section_47,
                test_case_section_48,
                test_case_section_49,
                test_case_section_50,
            ]
            .to_vec(),
        };
        test_url_extraction(&test_case, &engine);
    }

    #[test]
    fn test_goodhousekeeping_collection() {
        let (fetcher, scraper, extractor) = create_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSection {
            subtitle: "Cassis Manhattan",
            link:
                "https://www.goodhousekeeping.com/food-recipes/a61803557/cassis-manhattan-recipe/",
        };
        let test_case_section_2 = TestCaseSection {
            subtitle: "Black Margaritas with Torched Lime",
            link: "https://www.goodhousekeeping.com/food-recipes/a44911491/black-margaritas-with-torched-limes-recipe/",
        };
        let test_case_section_3 = TestCaseSection {
            subtitle: "Corpse Reviver No. 2",
            link: "https://www.goodhousekeeping.com/food-recipes/a61803460/corpse-reviver-no-2-recipe/",   
        };
        let test_case_section_4 = TestCaseSection {
            subtitle: "Pumpkin Spice White Russian",
            link: "https://www.goodhousekeeping.com/food-recipes/a41753485/pumpkin-spice-white-russian-recipe/",
        };
        let test_case_section_5 = TestCaseSection {
            subtitle: "Sparkling Pomegranate Punch",
            link: "https://www.goodhousekeeping.com/food-recipes/a45615860/sparkling-pomegranate-punch-recipe/",
        };
        let test_case_section_6 = TestCaseSection {
            subtitle: "Negroni",
            link:
                "https://www.goodhousekeeping.com/food-recipes/a40784252/negroni-cocktail-recipe/",
        };
        let test_case_section_7 = TestCaseSection {
            subtitle: "Witches' Brew Cocktail",
            link: "https://www.goodhousekeeping.com/food-recipes/a34331044/witches-brew-cocktail-recipe/",
        };
        let test_case_section_8 = TestCaseSection {
            subtitle: "Green Punch",
            link:
                "https://www.goodhousekeeping.com/food-recipes/a45597692/grinch-green-punch-recipe/",
        };
        let test_case_section_9 = TestCaseSection {
            subtitle: "Pomegranate Poison Spritz",
            link: "https://www.goodhousekeeping.com/food-recipes/a46067/pomegranate-poison-spritz-recipe/",
        };
        let test_case_section_10 = TestCaseSection {
            subtitle: "Spicy Margarita",
            link: "https://www.goodhousekeeping.com/food-recipes/party-ideas/a42296920/spicy-margarita-recipe/",
        };
        let test_case_section_11 = TestCaseSection {
            subtitle: "Eye-See-You Martini",
            link: "https://www.goodhousekeeping.com/food-recipes/party-ideas/a41530681/eye-see-you-martinis-recipe/",
        };
        let test_case_section_12 = TestCaseSection {
            subtitle: "Sparkling Pomegranate Cocktail",
            link: "https://www.goodhousekeeping.com/food-recipes/a37994321/sparkling-pomegranate-cocktail-recipe/",
        };
        let test_case_section_13 = TestCaseSection {
            subtitle: "Pear Gin Fizz",
            link: "https://www.goodhousekeeping.com/food-recipes/a38474802/pear-gin-fizz-recipe/",
        };
        let test_case_section_14 = TestCaseSection {
            subtitle: "Bees Knees",
            link: "https://www.goodhousekeeping.com/food-recipes/a37532647/bees-knees-cocktail-recipe/",
        };
        let test_case_section_15 = TestCaseSection {
            subtitle: "Rosemary Gin Gimlet",
            link: "https://www.goodhousekeeping.com/food-recipes/a37116577/rosemary-gin-gimlet-recipe/",
        };
        let test_case_section_16 = TestCaseSection {
            subtitle: "Cherry Simple Syrup",
            link: "https://www.goodhousekeeping.com/food-recipes/a36688389/cherry-simple-syrup-recipe/",
        };
        let test_case_section_17 = TestCaseSection {
            subtitle: "Green Beer",
            link: "https://www.goodhousekeeping.com/food-recipes/a37082/how-to-make-green-beer/",
        };
        let test_case_section_18 = TestCaseSection {
            subtitle: "Swamp Thing Halloween Cocktail",
            link: "https://www.goodhousekeeping.com/food-recipes/a28553276/swamp-thing-recipe/",
        };
        let test_case_section_19 = TestCaseSection {
            subtitle: "Beer Cocktails",
            link: "https://www.goodhousekeeping.com/food-recipes/a28408731/beer-cocktails-recipe/",
        };
        let test_case_section_20 = TestCaseSection {
            subtitle: "Cherry Crush Halloween Cocktail",
            link: "https://www.goodhousekeeping.com/food-recipes/a28552706/cherry-crush-recipe/",
        };
        let test_case_section_21 = TestCaseSection {
            subtitle: "Shirley Temple Drink",
            link: "https://www.goodhousekeeping.com/food-recipes/a29343552/shirley-temple-drink-recipe/",
        };
        let test_case_section_22 = TestCaseSection {
            subtitle: "Sparkling Ginger Sangria",
            link: "https://www.goodhousekeeping.com/food-recipes/a28554897/sparkling-ginger-sangria-recipe/",
        };
        let test_case_section_23 = TestCaseSection {
            subtitle: "Cherry Sidecar",
            link: "https://www.goodhousekeeping.com/food-recipes/a36890272/cherry-sidecar-recipe/",
        };
        let test_case_section_24 = TestCaseSection {
            subtitle: "Black Charcoal Lemonade Halloween Cocktail",
            link: "https://www.goodhousekeeping.com/food-recipes/a28552458/black-charcoal-lemonade-recipe/",
        };
        let test_case = TestCase {
            url: "https://www.goodhousekeeping.com/holidays/halloween-ideas/g3718/best-halloween-cocktails/",
            title: "24 Frightful Halloween Cocktails to Make for Your Spooky Bash",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
                test_case_section_14,
                test_case_section_15,
                test_case_section_16,
                test_case_section_17,
                test_case_section_18,
                test_case_section_19,
                test_case_section_20,
                test_case_section_21,
                test_case_section_22,
                test_case_section_23,
                test_case_section_24,
            ].to_vec(),
        };
        test_url_extraction(&test_case, &engine);
    }

    #[test]
    fn test_goodhousekeeping_collection_version_b_section_scoped() {
        let (fetcher, scraper, extractor) = create_section_scoped_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Cassis Manhattan",
            link:
                "https://www.goodhousekeeping.com/food-recipes/a61803557/cassis-manhattan-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/4x6-template-8-66ccf1a06554d.png",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Black Margaritas with Torched Lime",
            link: "https://www.goodhousekeeping.com/food-recipes/a44911491/black-margaritas-with-torched-limes-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/black-margaritas-with-torched-limes-6500e01677655.jpg",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Corpse Reviver No. 2",
            link: "https://www.goodhousekeeping.com/food-recipes/a61803460/corpse-reviver-no-2-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/4x6-template-9-66ccf31b8cdab.png",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Pumpkin Spice White Russian",
            link: "https://www.goodhousekeeping.com/food-recipes/a41753485/pumpkin-spice-white-russian-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-cocktails-pumpkin-spice-white-russian-64e8d0ed2f2c8.jpg",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Sparkling Pomegranate Punch",
            link: "https://www.goodhousekeeping.com/food-recipes/a45615860/sparkling-pomegranate-punch-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/4x6-template-11-66ccf502660db.png",
        };
        let test_case_section_6 = TestCaseSectionWithImage {
            subtitle: "Negroni",
            link: "https://www.goodhousekeeping.com/food-recipes/a40784252/negroni-cocktail-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-cocktails-negroni-64e8d828661ab.jpg",
        };
        let test_case_section_7 = TestCaseSectionWithImage {
            subtitle: "Witches' Brew Cocktail",
            link: "https://www.goodhousekeeping.com/food-recipes/a34331044/witches-brew-cocktail-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/ghk100120halloween-025-1602262184.jpg",
        };
        let test_case_section_8 = TestCaseSectionWithImage {
            subtitle: "Green Punch",
            link:
                "https://www.goodhousekeeping.com/food-recipes/a45597692/grinch-green-punch-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/4x6-template-12-66ccf62c9b8ad.png",
        };
        let test_case_section_9 = TestCaseSectionWithImage {
            subtitle: "Pomegranate Poison Spritz",
            link: "https://www.goodhousekeeping.com/food-recipes/a46067/pomegranate-poison-spritz-recipe/",
            image: "https://hips.hearstapps.com/goodhousekeeping/assets/17/38/halloween-cocktail.jpg",
        };
        let test_case_section_10 = TestCaseSectionWithImage {
            subtitle: "Spicy Margarita",
            link: "https://www.goodhousekeeping.com/food-recipes/party-ideas/a42296920/spicy-margarita-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-cocktails-spicy-margarita-64e8d51b1791d.jpg",
        };
        let test_case_section_11 = TestCaseSectionWithImage {
            subtitle: "Eye-See-You Martini",
            link: "https://www.goodhousekeeping.com/food-recipes/party-ideas/a41530681/eye-see-you-martinis-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/eye-see-you-martinis-1665002378.jpg",
        };
        let test_case_section_12 = TestCaseSectionWithImage {
            subtitle: "Sparkling Pomegranate Cocktail",
            link: "https://www.goodhousekeeping.com/food-recipes/a37994321/sparkling-pomegranate-cocktail-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-cocktails-sparkling-pomegranate-cocktail-1654035056.jpeg",
        };
        let test_case_section_13 = TestCaseSectionWithImage {
            subtitle: "Pear Gin Fizz",
            link: "https://www.goodhousekeeping.com/food-recipes/a38474802/pear-gin-fizz-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-cocktails-pear-gin-fizz-1654034564.jpeg",
        };
        let test_case_section_14 = TestCaseSectionWithImage {
            subtitle: "Bees Knees",
            link: "https://www.goodhousekeeping.com/food-recipes/a37532647/bees-knees-cocktail-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-cocktails-bees-knees-1654035475.jpeg",
        };
        let test_case_section_15 = TestCaseSectionWithImage {
            subtitle: "Rosemary Gin Gimlet",
            link: "https://www.goodhousekeeping.com/food-recipes/a37116577/rosemary-gin-gimlet-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/rosemary-gin-gimlet-1627485064.jpg",
        };
        let test_case_section_16 = TestCaseSectionWithImage {
            subtitle: "Cherry Simple Syrup",
            link: "https://www.goodhousekeeping.com/food-recipes/a36688389/cherry-simple-syrup-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/cherry-simple-syrup-1625150107.jpg",
        };
        let test_case_section_17 = TestCaseSectionWithImage {
            subtitle: "Green Beer",
            link: "https://www.goodhousekeeping.com/food-recipes/a37082/how-to-make-green-beer/",
            image: "https://hips.hearstapps.com/hmg-prod/images/green-beer-1625150373.jpg",
        };
        let test_case_section_18 = TestCaseSectionWithImage {
            subtitle: "Swamp Thing Halloween Cocktail",
            link: "https://www.goodhousekeeping.com/food-recipes/a28553276/swamp-thing-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-cocktails-swamp-thing-cocktail-1564502979.jpg",
        };
        let test_case_section_19 = TestCaseSectionWithImage {
            subtitle: "Beer Cocktails",
            link: "https://www.goodhousekeeping.com/food-recipes/a28408731/beer-cocktails-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/beer-cocktails-1625150542.jpg",
        };
        let test_case_section_20 = TestCaseSectionWithImage {
            subtitle: "Cherry Crush Halloween Cocktail",
            link: "https://www.goodhousekeeping.com/food-recipes/a28552706/cherry-crush-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-cocktails-cherry-crush-1633959591.jpeg",
        };
        let test_case_section_21 = TestCaseSectionWithImage {
            subtitle: "Shirley Temple Drink",
            link: "https://www.goodhousekeeping.com/food-recipes/a29343552/shirley-temple-drink-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/shirley-temple-drink-1625153282.jpg",
        };
        let test_case_section_22 = TestCaseSectionWithImage {
            subtitle: "Sparkling Ginger Sangria",
            link: "https://www.goodhousekeeping.com/food-recipes/a28554897/sparkling-ginger-sangria-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-cocktails-ginger-grape-sangria-1564511793.jpg",
        };
        let test_case_section_23 = TestCaseSectionWithImage {
            subtitle: "Cherry Sidecar",
            link: "https://www.goodhousekeeping.com/food-recipes/a36890272/cherry-sidecar-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/cherry-sidecar-1625153495.jpg",
        };
        let test_case_section_24 = TestCaseSectionWithImage {
            subtitle: "Black Charcoal Lemonade Halloween Cocktail",
            link: "https://www.goodhousekeeping.com/food-recipes/a28552458/black-charcoal-lemonade-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/black-charcoal-lemonade-1564499560.jpg",
        };
        let test_case = TestCaseWithImage {
            url: "https://www.goodhousekeeping.com/holidays/halloween-ideas/g3718/best-halloween-cocktails/",
            title: "24 Frightful Halloween Cocktails to Make for Your Spooky Bash",
            image: "https://hips.hearstapps.com/hmg-prod/images/black-margaritas-with-torched-limes-6500e01677655.jpg?crop=1.00xw:0.319xh;0,0.255xh&resize=1200:*",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
                test_case_section_14,
                test_case_section_15,
                test_case_section_16,
                test_case_section_17,
                test_case_section_18,
                test_case_section_19,
                test_case_section_20,
                test_case_section_21,
                test_case_section_22,
                test_case_section_23,
                test_case_section_24,
            ].to_vec(),
        };
        println!("\n🔬 A/B TEST VERSION B: Section-Scoped Good Housekeeping Collection");
        println!("Expected: This test should show how section-scoped performs vs regular scraping");
        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_bbcgoodfood_collection() {
        let (fetcher, scraper, extractor) = create_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSection {
            subtitle: "Haunting Halloween cocktail",
            link: "https://www.bbcgoodfood.com/recipes/halloween-cocktail",
        };
        let test_case_section_2 = TestCaseSection {
            subtitle: "Halloween punch",
            link: "https://www.bbcgoodfood.com/recipes/halloween-punch",
        };
        let test_case_section_3 = TestCaseSection {
            subtitle: "Salted caramel rum hot chocolate",
            link: "https://www.bbcgoodfood.com/recipes/salted-caramel-rum-hot-chocolate",
        };
        let test_case_section_4 = TestCaseSection {
            subtitle: "Spiced bloody Mary shots",
            link: "https://www.bbcgoodfood.com/recipes/spiced-bloody-mary-shots",
        };
        let test_case_section_5 = TestCaseSection {
            subtitle: "Nosferatini cocktail",
            link: "https://www.bbcgoodfood.com/recipes/nosferatini",
        };
        let test_case_section_6 = TestCaseSection {
            subtitle: "Grasshopper cocktail",
            link: "https://www.bbcgoodfood.com/recipes/grasshopper-cocktail",
        };
        let test_case_section_7 = TestCaseSection {
            subtitle: "Death in the afternoon",
            link: "https://www.bbcgoodfood.com/premium/death-in-the-afternoon",
        };
        let test_case_section_8 = TestCaseSection {
            subtitle: "Les fleurs du mal cocktail (the flowers of evil)",
            link: "https://www.bbcgoodfood.com/recipes/les-fleurs-du-mal-flowers-evil",
        };
        let test_case_section_9 = TestCaseSection {
            subtitle: "Rosita",
            link: "https://www.bbcgoodfood.com/premium/rosita-2",
        };
        let test_case_section_10 = TestCaseSection {
            subtitle: "Blood beetroot cocktails",
            link: "https://www.bbcgoodfood.com/recipes/blood-beetroot-cocktails",
        };
        let test_case_section_11 = TestCaseSection {
            subtitle: "Cherry sour",
            link: "https://www.bbcgoodfood.com/recipes/cherry-sour",
        };
        let test_case_section_12 = TestCaseSection {
            subtitle: "Zombie cocktail",
            link: "https://www.bbcgoodfood.com/recipes/zombie-cocktail",
        };
        let test_case_section_13 = TestCaseSection {
            subtitle: "Cranberry whiskey sour",
            link: "https://www.bbcgoodfood.com/recipes/cranberry-whiskey-sour",
        };
        let test_case_section_14 = TestCaseSection {
            subtitle: "Vampiro cocktail",
            link: "https://www.bbcgoodfood.com/recipes/vampiro",
        };
        let test_case_section_15 = TestCaseSection {
            subtitle: "Toffee apple sour cocktail",
            link: "https://www.bbcgoodfood.com/recipes/toffee-apple-sour",
        };
        let test_case_section_16 = TestCaseSection {
            subtitle: "Soul reviver cocktail",
            link: "https://www.bbcgoodfood.com/recipes/soul-reviver-cocktail",
        };
        let test_case_section_17 = TestCaseSection {
            subtitle: "Black russian cocktail",
            link: "https://www.bbcgoodfood.com/recipes/black-russian-cocktail",
        };
        let test_case_section_18 = TestCaseSection {
            subtitle: "Corpse reviver no. 2",
            link: "https://www.bbcgoodfood.com/recipes/corpse-reviver-no-2",
        };
        let test_case_section_19 = TestCaseSection {
            subtitle: "Witch's brew",
            link: "https://www.bbcgoodfood.com/recipes/witchs-brew",
        };
        let test_case_section_20 = TestCaseSection {
            subtitle: "Vampire's kiss",
            link: "https://www.bbcgoodfood.com/recipes/vampire-kiss",
        };
        let test_case = TestCase {
            url: "https://www.bbcgoodfood.com/howto/guide/top-10-halloween-cocktail-recipes",
            title: "Top 20 Halloween cocktail recipes",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
                test_case_section_14,
                test_case_section_15,
                test_case_section_16,
                test_case_section_17,
                test_case_section_18,
                test_case_section_19,
                test_case_section_20,
            ]
            .to_vec(),
        };
        test_url_extraction(&test_case, &engine);
    }

    #[test]
    fn test_absolut_collection() {
        let (fetcher, scraper, extractor) = create_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Spooky Pumpkin Martini",
            link: "https://www.absolutdrinks.com/en/drinks/spooky-pumpkin-martini/",
            image: "https://www.absolutdrinks.com/wp-content/uploads/recipe_spooky-pumpkin-martini_4x3_612358f90206cac4797aa20d2a655334.jpg",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Blood and Sand",
            link: "https://www.absolutdrinks.com/en/drinks/blood-and-sand/",
            image: "https://www.absolutdrinks.com/wp-content/uploads/recipe_blood-and-sand_1x1_086fde7371aae8dba007b29bd03e544e.jpg",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Bloody Mary Shot",
            link: "https://www.absolutdrinks.com/en/drinks/bloody-mary-shot/",
            image: "https://www.absolutdrinks.com/wp-content/uploads/recipe_bloody-mary-shot_1x1_d6b91b2aa8d2c28a3a4f1b630e87e987.jpg",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Spooky Colada",
            link: "https://www.absolutdrinks.com/en/drinks/spooky-colada/",
            image: "https://www.absolutdrinks.com/wp-content/uploads/recipe_spooky-colada_1x1_1862950f7bd69fa4c5ba88c446eab326.jpg",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Corpse Reviver",
            link: "https://www.absolutdrinks.com/en/drinks/corpse-reviver/",
            image: "https://www.absolutdrinks.com/wp-content/uploads/recipe_corpse-reviver_1x1_be40d0dad5e22db1f74233b896db818a.jpg",
        };
        let test_case = TestCaseWithImage {
            url: "https://www.absolut.com/en/blog/cocktails-and-mixology/5-spooky-halloween-cocktail-recipes/",
            title: "5 Spooky Halloween Cocktail Recipes",
            image: "https://www.absolut.com/cdn-cgi/image/format=auto,quality=55,width=3840,aspectRatio=square/wp-content/uploads/halloween-cocktails-5-easy-to-make.webp",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
            ].to_vec()
        };
        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_a_couple_cooks_collection() {
        let (fetcher, scraper, extractor) = create_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Witches Brew Drink",
            link: "https://www.acouplecooks.com/witches-brew-drink/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2021/08/Witchs-Brew-Cocktail-001.jpg",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Vampire's Kiss Cocktail",
            link: "https://www.acouplecooks.com/vampires-kiss-cocktail/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2021/08/Vampires-Kiss-Cocktail-005.jpg",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Halloween Margarita",
            link: "https://www.acouplecooks.com/halloween-margarita/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2023/09/Halloween-Margarita-005.jpg",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Halloween Punch",
            link: "https://www.acouplecooks.com/halloween-punch/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2021/08/Halloween-Punch-005.jpg",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Halloween Sangria",
            link: "https://www.acouplecooks.com/halloween-sangria/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2023/09/Halloween-Sangria-014.jpg",
        };
        let test_case_section_6 = TestCaseSectionWithImage {
            subtitle: "Apple Cider Spritz",
            link: "https://www.acouplecooks.com/apple-cider-spritz/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2024/09/Apple-Cider-Spritz-0005.jpg",
        };
        let test_case_section_7 = TestCaseSectionWithImage {
            subtitle: "Drunk Ghost",
            link: "https://www.shakedrinkrepeat.com/drunk-ghost/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2023/09/drunk-ghost-HERO-1200x1800.jpg",
        };
        let test_case_section_8 = TestCaseSectionWithImage {
            subtitle: "Butterbeer",
            link: "https://www.acouplecooks.com/butterbeer/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2023/10/Butter-Beer-011.jpg",
        };
        let test_case_section_9 = TestCaseSectionWithImage {
            subtitle: "Skeleton Key Cocktail",
            link: "https://www.acouplecooks.com/skeleton-key-cocktail/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2023/09/Skelton-Key-Cocktail-001.jpg",
        };
        let test_case_section_10 = TestCaseSectionWithImage {
            subtitle: "Corpse Reviver",
            link: "https://www.acouplecooks.com/corpse-reviver/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2020/08/Corpse-Reviver-No-2-001.jpg",
        };
        let test_case_section_11 = TestCaseSectionWithImage {
            subtitle: "Nightmare on Bourbon Street",
            link: "https://www.halfbakedharvest.com/nightmare-on-bourbon-street/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2022/10/Nightmare-on-Bourbon-Street-1.jpg",
        };
        let test_case_section_12 = TestCaseSectionWithImage {
            subtitle: "Pumpkin Martini",
            link: "https://www.acouplecooks.com/pumpkin-martini/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2021/09/Pumpkin-Pie-Martini-006.jpg",
        };
        let test_case_section_13 = TestCaseSectionWithImage {
            subtitle: "Death in the Afternoon Cocktail",
            link: "https://www.acouplecooks.com/death-in-the-afternoon-cocktail/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2020/12/Death-in-the-afternoon-008.jpg",
        };
        let test_case_section_14 = TestCaseSectionWithImage {
            subtitle: "Haunted Pumpkin Patch Margarita",
            link: "https://www.halfbakedharvest.com/pumpkin-patch-margarita/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2022/10/Haunted-Pumpkin-Patch-Margarita-1.jpg",
        };
        let test_case_section_15 = TestCaseSectionWithImage {
            subtitle: "Pumpkin Old Fashioned",
            link: "https://www.acouplecooks.com/pumpkin-old-fashioned-cocktail/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2021/08/Pumpkin-Cocktail-Pumpkin-Old-Fashioned-007.jpg",
        };
        let test_case_section_16 = TestCaseSectionWithImage {
            subtitle: "Poisoned Apple Cocktail",
            link: "https://www.sweetteaandthyme.com/poison-apple-cocktail/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2023/09/poisoned-apple-cocktail-hero-Edit.jpgfit12002c1500ssl1.jpg",
        };
        let test_case_section_17 = TestCaseSectionWithImage {
            subtitle: "The Zombie",
            link: "https://www.acouplecooks.com/zombie-cocktail/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2020/11/Zombie-Cocktail-005.jpg",
        };
        let test_case_section_18 = TestCaseSectionWithImage {
            subtitle: "Apple Cider Martini",
            link: "https://www.acouplecooks.com/apple-cider-martini/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2021/09/Apple-Cider-Martini-005.jpg",
        };
        let test_case_section_19 = TestCaseSectionWithImage {
            subtitle: "Sour Frankenstein Halloween Cocktail",
            link: "https://thegirlonbloor.com/sour-frankenstein-cocktail/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2023/09/Sour-Frankenstein-Cocktail-5.jpg",
        };
        let test_case_section_20 = TestCaseSectionWithImage {
            subtitle: "Bloody Mary",
            link: "https://www.acouplecooks.com/bloody-mary-recipe/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2020/06/Bloody-Mary-Recipe-010.jpg",
        };
        let test_case_section_21 = TestCaseSectionWithImage {
            subtitle: "Apple Cider Mule",
            link: "https://www.acouplecooks.com/apple-cider-mule/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2020/08/Apple-Cider-Mule-002.jpg",
        };
        let test_case_section_22 = TestCaseSectionWithImage {
            subtitle: "The Grave Digger",
            link: "https://www.halfbakedharvest.com/the-grave-digger/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2022/10/The-Grave-Digger-6.jpg",
        };
        let test_case_section_23 = TestCaseSectionWithImage {
            subtitle: "Blood and Sand",
            link: "https://www.acouplecooks.com/blood-and-sand-cocktail/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2020/07/Blood-and-Sand-Cocktail-006.jpg",
        };
        let test_case_section_24 = TestCaseSectionWithImage {
            subtitle: "Hot Buttered Rum",
            link: "https://www.acouplecooks.com/hot-buttered-rum/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2020/09/Buttered-Rum-006.jpg",
        };
        let test_case_section_25 = TestCaseSectionWithImage {
            subtitle: "Pumpkin Spice White Russian",
            link: "https://www.thecookierookie.com/pumpkin-spice-white-russian-cocktail/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2024/09/pumpkin-spice-white-russian-5-of-13.jpg",
        };
        let test_case_section_26 = TestCaseSectionWithImage {
            subtitle: "Dark and Stormy",
            link: "https://www.acouplecooks.com/dark-and-stormy-cocktail/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2020/08/Dark-and-Stormy-004.jpg",
        };
        let test_case_section_27 = TestCaseSectionWithImage {
            subtitle: "Fireball and Apple Cider",
            link: "https://www.acouplecooks.com/fireball-and-apple-cider/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2021/09/Fireball-and-Apple-Cider-003.jpg",
        };
        let test_case_section_28 = TestCaseSectionWithImage {
            subtitle: "Mulled Cider",
            link: "https://www.acouplecooks.com/mulled-cider/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2019/11/Mulled-Cider-003-1.jpg",
        };
        let test_case_section_29 = TestCaseSectionWithImage {
            subtitle: "Blood Orange Margarita",
            link: "https://www.acouplecooks.com/blood-orange-margarita/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2021/01/Blood-Orange-Margarita-008.jpg",
        };
        let test_case_section_30 = TestCaseSectionWithImage {
            subtitle: "Halloween Mimosa",
            link: "https://fatgirlhedonist.com/halloween-mimosa/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2023/09/IMG_0169-2-1200x1800.jpg",
        };
        let test_case = TestCaseWithImage {
            url: "https://www.acouplecooks.com/halloween-cocktails-drinks/",
            title: "30 Halloween Cocktails & Drinks",
            image: "https://www.acouplecooks.com/wp-content/uploads/2021/08/Halloween-Cocktails-004.jpg",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
                test_case_section_14,
                test_case_section_15,
                test_case_section_16,
                test_case_section_17,
                test_case_section_18,
                test_case_section_19,
                test_case_section_20,
                test_case_section_21,
                test_case_section_22,
                test_case_section_23,
                test_case_section_24,
                test_case_section_25,
                test_case_section_26,
                test_case_section_27,
                test_case_section_28,
                test_case_section_29,
                test_case_section_30,
            ]
            .to_vec(),
        };
        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_foodnetwork_collection() {
        let (fetcher, scraper, extractor) = create_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Boo-zy Halloween Cocktails",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/blueberry-rickety-eyeball-punch-recipe-2108558",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2012/7/26/0/641410_Blueberry-Rickety-Eyeball-Punch_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1371607504211.webp",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Witch’s Brew",
            link: "https://www.foodnetwork.com/recipes/sandra-lee/witchs-brew-recipe-2125712",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2013/5/10/0/SH1B26_Witchs-Brew_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1436539538681.webp",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Poison Apple Punch",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/poison-apple-punch-3853863",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2017/8/28/1/FNM100117_Poison-Apple-Punch_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1503956613322.webp",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Blood Orange Vampire Punch",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/blood-orange-vampire-punch-3853959",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2017/8/28/0/FNM100117_Blood-Orange-Vampire-Punch_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1503950700174.webp",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Black Light Cocktail",
            link: "https://www.foodnetwork.com/recipes/trisha-yearwood/black-light-cocktail-3893920",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2017/8/7/0/YW1008H_Black-Light-Cocktail_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1502140323625.webp",
        };
        let test_case_section_6 = TestCaseSectionWithImage {
            subtitle: "Ghost Cocktails",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/ghost-cocktails-12741318",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2022/07/15/0/FNK_Ghost-Cocktails_H1_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1657896865680.webp",
        };
        let test_case_section_7 = TestCaseSectionWithImage {
            subtitle: "Pumpkin Sangria",
            link: "https://www.foodnetwork.com/recipes/sandra-lee/pumpkin-sangria-recipe-2014994",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2011/10/25/2/SHSP06H_Pumpkin-Sangria_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1379761947055.webp",
        };
        let test_case_section_8 = TestCaseSectionWithImage {
            subtitle: "Hemlock Cocktails",
            link: "https://www.foodnetwork.com/recipes/hemlock-cocktails-2639671",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2014/9/9/0/RF0311_Hemlock-Cocktail_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1413396485200.webp",
        };
        let test_case_section_9 = TestCaseSectionWithImage {
            subtitle: "Bubbling Cauldron Punch",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/bubbling-cauldron-punch-3853940",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2017/8/28/0/FNM100117_Bubbling-Cauldron-Punch_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1503947570488.webp",
        };
        let test_case_section_10 = TestCaseSectionWithImage {
            subtitle: "Dark and Spooky",
            link: "https://www.foodnetwork.com/recipes/claire-robinson/claire-robinsons-dark-and-spooky-recipe-1972942",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2009/8/13/1/FNM100109Party003_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1371591304292.webp",
        };
        let test_case_section_11 = TestCaseSectionWithImage {
            subtitle: "Berry Eyeball Punch",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/berry-eyeball-punch-3853960",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2017/8/28/0/FNM100117_Berry-Eyeball-Punch_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1503947578075.webp",
        };
        let test_case_section_12 = TestCaseSectionWithImage {
            subtitle: "Halloween Jell-O Shots",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/halloween-jell-o-shots-12723353",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2022/07/15/0/FNK_Halloween-Jell-O-Shots_H2_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1657896818004.webp",
        };
        let test_case_section_13 = TestCaseSectionWithImage {
            subtitle: "Zombie Punch",
            link: "https://www.foodnetwork.com/recipes/patricia-heaton/zombie-punch-3141722",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2015/10/16/2/PP0101_Zombie-Punch_s4x3.jpg.rend.hgtvcom.1280.1024.85.suffix/1445032712088.webp",
        };
        let test_case_section_14 = TestCaseSectionWithImage {
            subtitle: "Candy Corn Cordials",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/candy-corn-cordials-recipe-1972909",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2009/8/13/4/FNM100109CandyCorn006b_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1371590972627.webp",
        };
        let test_case_section_15 = TestCaseSectionWithImage {
            subtitle: "Blood-Red Cherry Punch",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/blood-red-cherry-punch-recipe-2109061",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2012/9/4/3/FNM_100112-Vampire-Party-Bloody-Punch-Bowl-Blood-Red-Cherry-Punch-002_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1371609657490.webp",
        };
        let test_case_section_16 = TestCaseSectionWithImage {
            subtitle: "Phoenix Rising Cocktail",
            link: "https://www.foodnetwork.com/recipes/sandra-lee/phoenix-rising-cocktail-recipe-1926119",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2011/11/22/0/SHSP06_phoenix-rising-cocktail_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1379761950210.webp",
        };
        let test_case_section_17 = TestCaseSectionWithImage {
            subtitle: "Dragon's Blood Punch",
            link: "https://www.foodnetwork.com/recipes/sandra-lee/dragons-blood-punch-non-alcoholic-recipe-1950200",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2007/10/22/0/SH0907_Dragon_Blood_Punch.jpg.rend.hgtvcom.1280.960.85.suffix/1371585755612.webp",
        };
        let test_case_section_18 = TestCaseSectionWithImage {
            subtitle: "Sour Patch Sour",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/sour-patch-sour-17329174",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2023/8/22/FNM100123_sour-patch-sour_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1692738669049.webp",
        };
        let test_case_section_19 = TestCaseSectionWithImage {
            subtitle: "Black Cloud Cocktail",
            link: "https://www.foodnetwork.com/recipes/sandra-lee/black-cloud-cocktail-recipe-1960360",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2010/9/30/0/Sandra-Lee_Cocktail-02_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1371595580738.webp",
        };
        let test_case_section_20 = TestCaseSectionWithImage {
            subtitle: "Cider Fall Fireball",
            link: "https://www.foodnetwork.com/recipes/trisha-yearwood/cider-fall-fireball-5457163",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2018/10/5/0/YWSP01_Cocktails_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1538762386320.webp",
        };
        let test_case = TestCaseWithImage {
            url: "https://www.foodnetwork.com/holidays-and-parties/packages/halloween/halloween-drinks",
            title: "20 Haunted Cocktails To Serve at Your Halloween Party",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2012/7/26/0/641410_Blueberry-Rickety-Eyeball-Punch_s4x3.jpg.rend.hgtvcom.616.462.85.suffix/1371607504211.webp",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
                test_case_section_14,
                test_case_section_15,
                test_case_section_16,
                test_case_section_17,
                test_case_section_18,
                test_case_section_19,
                test_case_section_20,
            ].to_vec()
        };
        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_foodnetwork_collection_version_b_section_scoped() {
        // A/B Test Version B: Section-scoped approach
        // This should fix the section 17 link extraction bug by using section-scoped processing

        let (fetcher, scraper, extractor) = create_section_scoped_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };

        // Use identical test data as Version A for direct comparison
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Boo-zy Halloween Cocktails",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/blueberry-rickety-eyeball-punch-recipe-2108558",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2012/7/26/0/641410_Blueberry-Rickety-Eyeball-Punch_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1371607504211.webp",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Witch’s Brew",
            link: "https://www.foodnetwork.com/recipes/sandra-lee/witchs-brew-recipe-2125712",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2013/5/10/0/SH1B26_Witchs-Brew_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1436539538681.webp",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Poison Apple Punch",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/poison-apple-punch-3853863",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2017/8/28/1/FNM100117_Poison-Apple-Punch_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1503956613322.webp",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Blood Orange Vampire Punch",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/blood-orange-vampire-punch-3853959",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2017/8/28/0/FNM100117_Blood-Orange-Vampire-Punch_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1503950700174.webp",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Black Light Cocktail",
            link: "https://www.foodnetwork.com/recipes/trisha-yearwood/black-light-cocktail-3893920",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2017/8/7/0/YW1008H_Black-Light-Cocktail_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1502140323625.webp",
        };
        let test_case_section_6 = TestCaseSectionWithImage {
            subtitle: "Ghost Cocktails",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/ghost-cocktails-12741318",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2022/07/15/0/FNK_Ghost-Cocktails_H1_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1657896865680.webp",
        };
        let test_case_section_7 = TestCaseSectionWithImage {
            subtitle: "Pumpkin Sangria",
            link: "https://www.foodnetwork.com/recipes/sandra-lee/pumpkin-sangria-recipe-2014994",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2011/10/25/2/SHSP06H_Pumpkin-Sangria_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1379761947055.webp",
        };
        let test_case_section_8 = TestCaseSectionWithImage {
            subtitle: "Hemlock Cocktails",
            link: "https://www.foodnetwork.com/recipes/hemlock-cocktails-2639671",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2014/9/9/0/RF0311_Hemlock-Cocktail_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1413396485200.webp",
        };
        let test_case_section_9 = TestCaseSectionWithImage {
            subtitle: "Bubbling Cauldron Punch",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/bubbling-cauldron-punch-3853940",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2017/8/28/0/FNM100117_Bubbling-Cauldron-Punch_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1503947570488.webp",
        };
        let test_case_section_10 = TestCaseSectionWithImage {
            subtitle: "Dark and Spooky",
            link: "https://www.foodnetwork.com/recipes/claire-robinson/claire-robinsons-dark-and-spooky-recipe-1972942",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2009/8/13/1/FNM100109Party003_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1371591304292.webp",
        };
        let test_case_section_11 = TestCaseSectionWithImage {
            subtitle: "Berry Eyeball Punch",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/berry-eyeball-punch-3853960",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2017/8/28/0/FNM100117_Berry-Eyeball-Punch_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1503947578075.webp",
        };
        let test_case_section_12 = TestCaseSectionWithImage {
            subtitle: "Halloween Jell-O Shots",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/halloween-jell-o-shots-12723353",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2022/07/15/0/FNK_Halloween-Jell-O-Shots_H2_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1657896818004.webp",
        };
        let test_case_section_13 = TestCaseSectionWithImage {
            subtitle: "Zombie Punch",
            link: "https://www.foodnetwork.com/recipes/patricia-heaton/zombie-punch-3141722",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2015/10/16/2/PP0101_Zombie-Punch_s4x3.jpg.rend.hgtvcom.1280.1024.85.suffix/1445032712088.webp",
        };
        let test_case_section_14 = TestCaseSectionWithImage {
            subtitle: "Candy Corn Cordials",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/candy-corn-cordials-recipe-1972909",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2009/8/13/4/FNM100109CandyCorn006b_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1371590972627.webp",
        };
        let test_case_section_15 = TestCaseSectionWithImage {
            subtitle: "Blood-Red Cherry Punch",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/blood-red-cherry-punch-recipe-2109061",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2012/9/4/3/FNM_100112-Vampire-Party-Bloody-Punch-Bowl-Blood-Red-Cherry-Punch-002_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1371609657490.webp",
        };
        let test_case_section_16 = TestCaseSectionWithImage {
            subtitle: "Phoenix Rising Cocktail",
            link: "https://www.foodnetwork.com/recipes/sandra-lee/phoenix-rising-cocktail-recipe-1926119",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2011/11/22/0/SHSP06_phoenix-rising-cocktail_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1379761950210.webp",
        };
        // CRITICAL TEST CASE: Section 17 - this fails in Version A but should pass in Version B
        let test_case_section_17 = TestCaseSectionWithImage {
            subtitle: "Dragon's Blood Punch",
            link: "https://www.foodnetwork.com/recipes/sandra-lee/dragons-blood-punch-non-alcoholic-recipe-1950200",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2007/10/22/0/SH0907_Dragon_Blood_Punch.jpg.rend.hgtvcom.1280.960.85.suffix/1371585755612.webp",
        };
        let test_case_section_18 = TestCaseSectionWithImage {
            subtitle: "Sour Patch Sour",
            link: "https://www.foodnetwork.com/recipes/food-network-kitchen/sour-patch-sour-17329174",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2023/8/22/FNM100123_sour-patch-sour_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1692738669049.webp",
        };
        let test_case_section_19 = TestCaseSectionWithImage {
            subtitle: "Black Cloud Cocktail",
            link: "https://www.foodnetwork.com/recipes/sandra-lee/black-cloud-cocktail-recipe-1960360",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2010/9/30/0/Sandra-Lee_Cocktail-02_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1371595580738.webp",
        };
        let test_case_section_20 = TestCaseSectionWithImage {
            subtitle: "Cider Fall Fireball",
            link: "https://www.foodnetwork.com/recipes/trisha-yearwood/cider-fall-fireball-5457163",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2018/10/5/0/YWSP01_Cocktails_s4x3.jpg.rend.hgtvcom.1280.960.85.suffix/1538762386320.webp",
        };
        let test_case = TestCaseWithImage {
            url: "https://www.foodnetwork.com/holidays-and-parties/packages/halloween/halloween-drinks",
            title: "20 Haunted Cocktails To Serve at Your Halloween Party",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2012/7/26/0/641410_Blueberry-Rickety-Eyeball-Punch_s4x3.jpg.rend.hgtvcom.616.462.85.suffix/1371607504211.webp",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
                test_case_section_14,
                test_case_section_15,
                test_case_section_16,
                test_case_section_17, // KEY: Should pass with section-scoped approach
                test_case_section_18,
                test_case_section_19,
                test_case_section_20,
            ].to_vec()
        };

        println!("\n🔬 A/B TEST VERSION B: Section-Scoped Food Network Collection");
        println!("Expected: Section 17 'Dragon's Blood Punch' should extract correct link");
        println!("Critical fix: Section-scoped processing should find correct recipe link instead of competing page link");

        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_a_couple_cooks_collection_version_b_section_scoped() {
        // A/B Test Version B: Section-scoped approach for A Couple Cooks
        // Test regression safety - this collection should work well with both approaches

        let (fetcher, scraper, extractor) = create_section_scoped_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };

        // Use identical test data as Version A for direct comparison
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Witches Brew Drink",
            link: "https://www.acouplecooks.com/witches-brew-drink/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2021/08/Witchs-Brew-Cocktail-001.jpg",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Vampire's Kiss Cocktail",
            link: "https://www.acouplecooks.com/vampires-kiss-cocktail/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2021/08/Vampires-Kiss-Cocktail-005.jpg",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Halloween Margarita",
            link: "https://www.acouplecooks.com/halloween-margarita/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2023/09/Halloween-Margarita-005.jpg",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Halloween Punch",
            link: "https://www.acouplecooks.com/halloween-punch/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2021/08/Halloween-Punch-005.jpg",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Halloween Sangria",
            link: "https://www.acouplecooks.com/halloween-sangria/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2023/09/Halloween-Sangria-014.jpg",
        };
        let test_case_section_6 = TestCaseSectionWithImage {
            subtitle: "Apple Cider Spritz",
            link: "https://www.acouplecooks.com/apple-cider-spritz/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2024/09/Apple-Cider-Spritz-0005.jpg",
        };
        let test_case_section_7 = TestCaseSectionWithImage {
            subtitle: "Drunk Ghost",
            link: "https://www.shakedrinkrepeat.com/drunk-ghost/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2023/09/drunk-ghost-HERO-1200x1800.jpg",
        };
        let test_case_section_8 = TestCaseSectionWithImage {
            subtitle: "Butterbeer",
            link: "https://www.acouplecooks.com/butterbeer/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2023/10/Butter-Beer-011.jpg",
        };
        let test_case_section_9 = TestCaseSectionWithImage {
            subtitle: "Pomegranate Juice Cocktail",
            link: "https://www.acouplecooks.com/pomegranate-juice-cocktail/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2022/01/Pomegranate-Cocktail-009.jpg",
        };
        let test_case_section_10 = TestCaseSectionWithImage {
            subtitle: "Cranberry Mule",
            link: "https://www.acouplecooks.com/cranberry-mule/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2020/10/Cranberry-Mule-003.jpg",
        };
        let test_case_section_11 = TestCaseSectionWithImage {
            subtitle: "Sidecar",
            link: "https://www.acouplecooks.com/sidecar-cocktail/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2020/09/Sidecar-Cocktail-002.jpg",
        };
        let test_case_section_12 = TestCaseSectionWithImage {
            subtitle: "Cognac Old Fashioned",
            link: "https://www.acouplecooks.com/cognac-old-fashioned/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2023/03/Cognac-Old-Fashioned-008.jpg",
        };
        let test_case_section_13 = TestCaseSectionWithImage {
            subtitle: "Brandy Cocktails",
            link: "https://www.acouplecooks.com/brandy-cocktails/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2020/09/Best-Brandy-Cocktails-003.jpg",
        };
        let test_case_section_14 = TestCaseSectionWithImage {
            subtitle: "Spooky Shirley Temple",
            link: "https://www.acouplecooks.com/spooky-shirley-temple/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2021/08/Spooky-Shirley-Temple-002.jpg",
        };
        let test_case_section_15 = TestCaseSectionWithImage {
            subtitle: "Halloween Mimosa",
            link: "https://fatgirlhedonist.com/halloween-mimosa/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2023/09/IMG_0169-2-1200x1800.jpg",
        };
        let test_case_section_16 = TestCaseSectionWithImage {
            subtitle: "Fall Harvest Punch",
            link: "https://www.acouplecooks.com/fall-harvest-punch/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2021/08/Fall-Harvest-Punch-001.jpg",
        };
        let test_case_section_17 = TestCaseSectionWithImage {
            subtitle: "Apple Cider Margarita",
            link: "https://www.acouplecooks.com/apple-cider-margarita/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2020/09/Apple-Cider-Margarita-005.jpg",
        };
        let test_case_section_18 = TestCaseSectionWithImage {
            subtitle: "The Witch Doctor",
            link: "https://cocktails.foodandwine.com/recipes/the-witch-doctor",
            image: "https://www.acouplecooks.com/wp-content/uploads/2023/09/witch-doctor-hero-1200x1800.jpg",
        };
        let test_case_section_19 = TestCaseSectionWithImage {
            subtitle: "Black Devil",
            link: "https://www.acouplecooks.com/black-devil/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2020/06/Black-Devil-010.jpg",
        };
        let test_case_section_20 = TestCaseSectionWithImage {
            subtitle: "Bloody Mary",
            link: "https://www.acouplecooks.com/bloody-mary-recipe/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2020/06/Bloody-Mary-Recipe-010.jpg",
        };
        let test_case_section_21 = TestCaseSectionWithImage {
            subtitle: "Apple Cider Mule",
            link: "https://www.acouplecooks.com/apple-cider-mule/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2020/08/Apple-Cider-Mule-002.jpg",
        };
        let test_case_section_22 = TestCaseSectionWithImage {
            subtitle: "The Grave Digger",
            link: "https://www.halfbakedharvest.com/the-grave-digger/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2022/10/The-Grave-Digger-6.jpg",
        };
        let test_case_section_23 = TestCaseSectionWithImage {
            subtitle: "Blood and Sand",
            link: "https://www.acouplecooks.com/blood-and-sand-cocktail/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2020/07/Blood-and-Sand-Cocktail-006.jpg",
        };
        let test_case_section_24 = TestCaseSectionWithImage {
            subtitle: "Hot Buttered Rum",
            link: "https://www.acouplecooks.com/hot-buttered-rum/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2020/09/Buttered-Rum-006.jpg",
        };
        let test_case_section_25 = TestCaseSectionWithImage {
            subtitle: "Pumpkin Spice White Russian",
            link: "https://www.thecookierookie.com/pumpkin-spice-white-russian-cocktail/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2024/09/pumpkin-spice-white-russian-5-of-13.jpg",
        };
        let test_case_section_26 = TestCaseSectionWithImage {
            subtitle: "Dark and Stormy",
            link: "https://www.acouplecooks.com/dark-and-stormy-cocktail/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2020/08/Dark-and-Stormy-004.jpg",
        };
        let test_case_section_27 = TestCaseSectionWithImage {
            subtitle: "Fireball and Apple Cider",
            link: "https://www.acouplecooks.com/fireball-and-apple-cider/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2021/09/Fireball-and-Apple-Cider-003.jpg",
        };
        let test_case_section_28 = TestCaseSectionWithImage {
            subtitle: "Mulled Cider",
            link: "https://www.acouplecooks.com/mulled-cider/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2019/11/Mulled-Cider-003-1.jpg",
        };
        let test_case_section_29 = TestCaseSectionWithImage {
            subtitle: "Blood Orange Margarita",
            link: "https://www.acouplecooks.com/blood-orange-margarita/",
            image: "https://www.acouplecooks.com/wp-content/uploads/2021/01/Blood-Orange-Margarita-008.jpg",
        };
        let test_case_section_30 = TestCaseSectionWithImage {
            subtitle: "Halloween Mimosa",
            link: "https://fatgirlhedonist.com/halloween-mimosa/",
            image:
                "https://www.acouplecooks.com/wp-content/uploads/2023/09/IMG_0169-2-1200x1800.jpg",
        };

        let test_case = TestCaseWithImage {
            url: "https://www.acouplecooks.com/halloween-cocktails-drinks/",
            title: "30 Halloween Cocktails & Drinks",
            image: "https://www.acouplecooks.com/wp-content/uploads/2021/08/Halloween-Cocktails-004.jpg",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
                test_case_section_14,
                test_case_section_15,
                test_case_section_16,
                test_case_section_17,
                test_case_section_18,
                test_case_section_19,
                test_case_section_20,
                test_case_section_21,
                test_case_section_22,
                test_case_section_23,
                test_case_section_24,
                test_case_section_25,
                test_case_section_26,
                test_case_section_27,
                test_case_section_28,
                test_case_section_29,
                test_case_section_30,
            ].to_vec(),
        };

        println!("\n🔬 A/B TEST VERSION B: Section-Scoped A Couple Cooks Collection");
        println!("Expected: This should demonstrate regression safety - working collection should continue to work well");

        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_the_spruce_eats_collection_version_b_section_scoped() {
        // A/B Test Version B: Section-scoped approach for The Spruce Eats
        // Test regression safety on another working collection

        let (fetcher, scraper, extractor) = create_section_scoped_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };

        // Use identical test data as Version A for direct comparison
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Jack-O-Lantern",
            link: "https://www.thespruceeats.com/jack-o-lantern-cocktail-recipe-759441",
            image: "https://www.thespruceeats.com/thmb/xnw_a3-0h2feEynmLYOQk064E_o=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/jack-o-lantern-cocktail-recipe-759441-hero-images-1-975cebfd5e294060be0ba8c713529c02.jpg",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Halloween Hypnotist",
            link: "https://www.thespruceeats.com/halloween-hpnotist-recipe-761076",
            image: "https://www.thespruceeats.com/thmb/YdeMWTySSzGOmg4x572UDtgXACE=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/halloween-hpnotist-recipe-761076-hero-01-2e666ba5cbd5439fae40e1cc65bdbabd.jpg",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Mad Eye Martini",
            link: "https://www.thespruceeats.com/mad-eye-martini-recipe-761104",
            image: "https://www.thespruceeats.com/thmb/qsy8Hier0ti6FYEkKy01D83cGak=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/mad-eye-martini-recipe-761104-hero-01-f0975f8d4d284df4b5d3e707e3ed80f5.jpg",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Blood and Sand",
            link: "https://www.thespruceeats.com/blood-and-sand-cocktail-recipe-761336",
            image: "https://www.thespruceeats.com/thmb/b26R1rJ6eOmgom9qRH3hdOtdcc4=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/blood-and-sand-cocktail-recipe-761336-hero-d8e91f5e13d342b5b7a8abe4be6c1f5d.jpg",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Vampire Kiss Martini",
            link: "https://www.thespruceeats.com/vampire-kiss-martini-recipe-761200",
            image: "https://www.thespruceeats.com/thmb/EJ5lfEhz8EqqdwsPJ__tqQmBo74=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/vampire-kiss-martini-recipe-761200-hero-01-062830abd98c470db5e4bc5fe327d3c3.jpg",
        };
        let test_case_section_6 = TestCaseSectionWithImage {
            subtitle: "Fright Night in the Grove",
            link: "https://www.thespruceeats.com/fright-night-in-the-grove-cocktail-760774",
            image: "https://www.thespruceeats.com/thmb/zfmasMziTSTJtHdHarJt2HQqsL4=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/fright-night-in-the-grove-cocktail-760774-hero-01-79c6ebd2ba954db1955b5dab2dce9a8d.jpg",
        };
        let test_case_section_7 = TestCaseSectionWithImage {
            subtitle: "Frog in a Blender",
            link: "https://www.thespruceeats.com/frog-in-a-blender-recipe-761055",
            image: "https://www.thespruceeats.com/thmb/ems_UwBmgL4NDdVBhYX8OFlYzr0=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/frog-in-a-blender-recipe-761055-hero-01-5c54cd20c9e77c0001cff921.jpg",
        };
        let test_case_section_8 = TestCaseSectionWithImage {
            subtitle: "Skeleton Key",
            link: "https://www.thespruceeats.com/skeleton-key-cocktail-recipe-761383",
            image: "https://www.thespruceeats.com/thmb/BRaiGVUEJ8naNTu4FelIM1uohGQ=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/skeleton-key-cocktail-recipe-761383-hero-02-5cdcd93be5d2413f9d3a7a8056964c6e.jpg",
        };
        let test_case_section_9 = TestCaseSectionWithImage {
            subtitle: "Black Widow",
            link: "https://www.thespruceeats.com/black-widow-recipe-761008",
            image: "https://www.thespruceeats.com/thmb/znNmBCCxukTDG4jq-Egb0Cus6rc=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/black-widow-recipe-761008-hero-01-070706a180d04aec9b6736fa1d1f3c19.jpg",
        };
        let test_case_section_10 = TestCaseSectionWithImage {
            subtitle: "Ghostbuster",
            link: "https://www.thespruceeats.com/ghostbuster-cocktail-recipe-759668",
            image: "https://www.thespruceeats.com/thmb/z1k7z8C2iuFIKR5J6JIhOBVeLXE=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/ghostbuster-cocktail-recipe-759668-hero-01-5264544fa57d4d9c8b331c1638e4d8fc.jpg",
        };
        let test_case_section_11 = TestCaseSectionWithImage {
            subtitle: "Zombie",
            link: "https://www.thespruceeats.com/zombie-cocktail-recipe-761643",
            image: "https://www.thespruceeats.com/thmb/oiRY5sLzLk-ytbcZjct6ovpva1g=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/zombie-cocktail-recipe-761643-Hero-5b7424e2c9e77c0050ec7160.jpg",
        };
        let test_case_section_12 = TestCaseSectionWithImage {
            subtitle: "Wolf Bite",
            link: "https://www.thespruceeats.com/wolf-bite-shot-recipe-759565",
            image: "https://www.thespruceeats.com/thmb/tY1ds2xKOxKQa1eYng0yjMRaIxM=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/Wolf-Bite-Shot-56a173203df78cf7726abe54.jpg",
        };
        let test_case_section_13 = TestCaseSectionWithImage {
            subtitle: "Candy Corn Shot",
            link: "https://www.thespruceeats.com/candy-corn-shooter-recipe-759614",
            image: "https://www.thespruceeats.com/thmb/J-pagoch9VRoQBIBDGykpMhc9yw=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/candy-corn-shooter-recipe-759614-hero-cdc381f64705418aa400900c0b79ab47.jpg",
        };
        let test_case = TestCaseWithImage {
            url: "https://www.thespruceeats.com/haunting-halloween-cocktails-759881",
            title: "Spectacular Halloween Cocktails to Spook Your Guests",
            image: "https://www.thespruceeats.com/thmb/RhpEpxyZy5wivA9kH3poaeW6aGY=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/black-widow-recipe-761008-hero-01-5c8801c7c9e77c0001a3e5c9.jpg",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
            ]
            .to_vec(),
        };

        println!("\n🔬 A/B TEST VERSION B: Section-Scoped The Spruce Eats Collection");
        println!("Expected: Another regression safety test - working collection should maintain performance");

        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_paperlesspost_collection_version_b_section_scoped() {
        let (fetcher, scraper, extractor) = create_section_scoped_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Corpse Reviver",
            link: "https://www.liquor.com/recipes/corpse-reviver-no-2/",
            image: "https://www.liquor.com/thmb/OTadfw0Hpd0LAnpbCR7KA1VyJxc=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/corpse-reviver-no2-1200x628-email-0459f27378f04eed99ba99044ca47f99.jpg",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Jekyll & Gin",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a44311/jekyll-gin-glowing-cocktails-glow-party-ideas/",
            image: "https://hips.hearstapps.com/del.h-cdn.co/assets/15/42/1024x512/landscape-1444928749-delish-glow-food-jekyll-gin-recipe.jpg?resize=1200:*",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Bloody Mary Syringes",
            link: "https://www.delish.com/cooking/recipe-ideas/a24132876/bloody-mary-syringes-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/bloody-mary-syringes-horizontal2-1540477593.jpg?crop=1.00xw:0.752xh;0,0.118xh&resize=1200:*",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Witches’ Brew Lemonade",
            link: "https://www.delish.com/holiday-recipes/halloween/a29178988/witches-brew-lemonade-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/witches-brew-lemonade-index-66eddb5580cee.jpg?crop=1.00xw:1.00xh;0,0&resize=1200:*",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Zombie’s Shrunken Head",
            link: "https://www.thespruceeats.com/zombie-cocktail-recipe-761643",
            image: "https://www.thespruceeats.com/thmb/oiRY5sLzLk-ytbcZjct6ovpva1g=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/zombie-cocktail-recipe-761643-Hero-5b7424e2c9e77c0050ec7160.jpg",
        };
        let test_case_section_6 = TestCaseSectionWithImage {
            subtitle: "Dracula’s Kiss",
            link: "https://www.thespruceeats.com/draculas-kiss-cherry-vodka-cola-761041",
            image: "https://www.thespruceeats.com/thmb/ufRSYEuPqXj7I18XkRXNawoywYM=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/draculas-kiss-cherry-vodka-cola-761041-hero-01-9ac496ad5fc94f0e920b21ed8d5f46e9.jpg",
        };
        let test_case_section_7 = TestCaseSectionWithImage {
            subtitle: "An American Werewolf in London Fog",
            link: "https://www.thrillist.com/drink/nation/london-fog-cocktail-recipe",
            image: "https://assets3.thrillist.com/v1/image/2748851/1200x600/scale;;webp=auto;jpeg_quality=85.jpg",
        };
        let test_case_section_8 = TestCaseSectionWithImage {
            subtitle: "The Candyman",
            link: "https://cookieandkate.com/bees-knees-cocktail-recipe/",
            image: "https://cookieandkate.com/images/2020/04/bees-knees-drink.jpg",
        };
        let test_case_section_9 = TestCaseSectionWithImage {
            subtitle: "The Walking Dead",
            link: "https://craftandcocktails.co/2015/10/31/walking-dead-a-halloween-cocktail/",
            image: "https://craftandcocktails.co/wp-content/uploads/2015/10/walking-dead-cocktail-3.jpg",
        };
        let test_case_section_10 = TestCaseSectionWithImage {
            subtitle: "Slimer’s Ectoplasm",
            link: "https://diycandy.com/ghostbusters-cocktail/",
            image: "https://diycandy.b-cdn.net/wp-content/uploads/2016/08/Slimer-Ectoplasm-slime-cocktail-e1470531855283.jpg",
        };
        let test_case_section_11 = TestCaseSectionWithImage {
            subtitle: "Bourbon Butterbeer",
            link: "https://www.gastronomblog.com/bourbon-butterbeer/",
            image:
                "https://www.gastronomblog.com/wp-content/uploads/2016/10/bourbonbutterbeer-6.jpg",
        };
        let test_case_section_12 = TestCaseSectionWithImage {
            subtitle: "Polyjuice Potion",
            link: "https://www.crowdedkitchen.com/polyjuice-potion-cocktail/",
            image: "https://www.crowdedkitchen.com/wp-content/uploads/2020/10/potion.jpg",
        };
        let test_case_section_13 = TestCaseSectionWithImage {
            subtitle: "Rosemary’s Baby Punch",
            link: "https://www.seriouseats.com/prime-meats-whiskey-aperol-punch-rosemary-baby",
            image: "https://www.seriouseats.com/thmb/IFtiGfNr1PE6gZV2kxkOggNrqlk=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/__opt__aboutcom__coeus__resources__content_migration__serious_eats__seriouseats.com__recipes__images__20111114-PrimeMeats-1-9b86b6f67a6a4914ab4c48d27e3331d1.jpg",
        };
        let test_case_section_14 = TestCaseSectionWithImage {
            subtitle: "Demagorgon’s Dinner",
            link: "https://www.mainespirits.com/recipes/demogorgons-dinner",
            image: "https://www.mainespirits.com/recipes/demogorgons-dinner",
        };
        let test_case_section_15 = TestCaseSectionWithImage {
            subtitle: "The Silver Bullet",
            link: "https://www.vice.com/en/article/this-cocktail-is-potent-enough-to-kill-a-werewolf-and-its-made-with-real-silver/",
            image: "https://munchies-images.vice.com/wp_upload/silver-bullet.jpg?resize=1000:*",
        };
        let test_case_section_16 = TestCaseSectionWithImage {
            subtitle: "Michael Meyer’s Lemon Drop",
            link: "https://www.iconiccocktail.com/products/meyer-lemon-drop",
            image: "https://www.iconiccocktail.com/cdn/shop/products/Iconic_Meyer_Lemon_Balm_10_of_15_dcaa2ba9-323b-49f4-8d95-74b0f2ef1750.jpg?v=1559849900",
        };
        let test_case_section_17 = TestCaseSectionWithImage {
            subtitle: "Manhattan Chainsaw Massacre",
            link: "https://www.seriouseats.com/cocktails-manhattan-recipe",
            image: "https://www.seriouseats.com/thmb/ADgVAVIRqXTxPlSnX9Xjntiv7m8=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/20230811-SEA-Manhattan-TwoBites-005-8e3a7657e623426d9625a25fb362bcd1.jpg",
        };
        let test_case_section_18 = TestCaseSectionWithImage {
            subtitle: "Cthulhu’s Mai-Tai",
            link: "https://www.epicurious.com/recipes/food/views/mai-tai-230577",
            image: "https://assets.epicurious.com/photos/6239dd8cfc699f0e516897df/16:9/w_1280,c_limit/MaiTai_RECIPE_031722_30061.jpg",
        };
        let test_case_section_19 = TestCaseSectionWithImage {
            subtitle: "The Love Witch’s Earl Grey Martini",
            link: "https://www.allrecipes.com/recipe/162319/earl-grey-martini/",
            image: "https://www.allrecipes.com/thmb/vbYUZSg1lEZjdkbjMZfgqefMlko=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/875004-97005ad8419545dca0c2f248864fdb4d.jpg",
        };
        let test_case_section_20 = TestCaseSectionWithImage {
            subtitle: "Freddy’s Dream Warriors",
            link: "https://everydayshortcuts.com/freddy-krueger-dream-warriors-cocktail/",
            image: "https://everydayshortcuts.com/wp-content/uploads/2022/07/freddy-kreuger-dream-warriors-cocktail.jpg",
        };
        let test_case = TestCaseWithImage {
            url: "https://www.paperlesspost.com/blog/halloween-cocktails-drinks/",
            title: "20 Spooky Halloween cocktails and drinks",
            image: "https://www.paperlesspost.com/blog/wp-content/uploads/Opt2_092622_Blog_HalloweenCocktails_01-hero.png",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
                test_case_section_14,
                test_case_section_15,
                test_case_section_16,
                test_case_section_17,
                test_case_section_18,
                test_case_section_19,
                test_case_section_20,
            ].to_vec(),
        };

        println!("\n🔬 A/B TEST VERSION B: Section-Scoped Paperless Post Collection");
        println!(
            "Expected: Regression safety test - working collection should maintain performance"
        );

        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_purewow_collection_version_b_section_scoped() {
        let (fetcher, scraper, extractor) = create_section_scoped_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Chewy Chocolate Chip Granola Bars",
            link: "https://www.purewow.com/recipes/chewy-chocolate-chip-granola-bars",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2022/08/chewy-chocolate-chip-granola-bars-recipe-FB.jpg",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Gluten-Free Flourless Cocoa Cookies",
            link: "https://www.purewow.com/recipes/gluten-free-flourless-cookies",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2017/10/flourless-chocolate-cookies-fb.jpg",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Homemade Cinnamon Applesauce",
            link: "https://www.purewow.com/recipes/homemade-cinnamon-applesauce-recipe",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2015/12/applesauce-400.jpg?fit=400%2C290",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Chocolate Chip Cookie Dough Dip",
            link: "https://www.purewow.com/recipes/chocolate-chip-cookie-dough-dip",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2016/10/cookiedip-400.png?fit=400%2C290",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Cookies-and-Cream Ice Pops",
            link: "https://www.purewow.com/recipes/cookies-and-cream-ice-pops-recipe",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2017/07/cookies-and-cream-pops-630-fb.jpg",
        };
        let test_case_section_6 = TestCaseSectionWithImage {
            subtitle: "Breakfast Hand Pies",
            link: "https://www.purewow.com/recipes/breakfast-hand-pies",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2014/03/poptartfb.jpg",
        };
        let test_case_section_7 = TestCaseSectionWithImage {
            subtitle: "Strawberry Oatmeal Bars",
            link: "https://www.wellplated.com/strawberry-oatmeal-bars/",
            image: "https://www.wellplated.com/wp-content/uploads/2016/03/Easy-Strawberry-Oatmeal-Bars.jpg",
        };
        let test_case_section_8 = TestCaseSectionWithImage {
            subtitle: "Mini Caramel Apples",
            link: "https://www.purewow.com/recipes/mini-caramel-apples",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/06/school-safe-treats_mini-caramel-apples.jpeg?fit=680%2C860",
        };
        let test_case_section_9 = TestCaseSectionWithImage {
            subtitle: "Vegan and Gluten-Free Baked Doughnuts",
            link: "https://www.purewow.com/recipes/baked-gluten-free-doughnuts",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2021/02/vegan-gluten-free-baked-doughnuts-recipe-fb.jpg",
        };
        let test_case_section_10 = TestCaseSectionWithImage {
            subtitle: "Magic Pancakes with Bananas, Eggs and Yogurt",
            link: "https://www.purewow.com/recipes/magic-pancakes-with-bananas-eggs-and-yogurt",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2022/04/daphne-oz-magic-pancakes-recipe-fb.jpg",
        };
        let test_case_section_11 = TestCaseSectionWithImage {
            subtitle: "Apple Cider Doughnut Holes",
            link: "https://www.purewow.com/recipes/apple-cider-doughnut-holes",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/06/school-safe-treats_apple-cider-doughnut-holes.jpeg?fit=680%2C860",
        };
        let test_case_section_12 = TestCaseSectionWithImage {
            subtitle: "Watercolor Doughnuts",
            link: "https://www.purewow.com/recipes/watercolor-doughnuts",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/05/watercolor-doughnuts-recipe-fb.jpg",
        };
        let test_case_section_13 = TestCaseSectionWithImage {
            subtitle: "5-Ingredient Frozen Yogurt Bites",
            link: "https://playswellwithbutter.com/5-ingredient-frozen-yogurt-bites/",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/06/school-safe-treats_FROZEN-YOGURT-BITES-1.jpg?fit=680%2C800",
        };
        let test_case_section_14 = TestCaseSectionWithImage {
            subtitle: "Snickerdoodle Lucky Charms Cookies",
            link: "https://iamafoodblog.com/snickerdoodle-lucky-charms-cookies/",
            image: "https://iamafoodblog.b-cdn.net/wp-content/uploads/2019/03/lucky-charms-snickerdoodles-8702w.jpg",
        };
        let test_case_section_15 = TestCaseSectionWithImage {
            subtitle: "Silly Apple Bites",
            link: "https://www.forkandbeans.com/2015/08/06/silly-apple-bites/",
            image: "https://www.forkandbeans.com/wp-content/uploads/2015/08/Silly-Apple-Bites.jpg",
        };
        let test_case_section_16 = TestCaseSectionWithImage {
            subtitle: "Mini Chocolate Chip Muffins",
            link: "https://eatwithclarity.com/mini-chocolate-chip-muffins/#wprm-recipe-container-33069",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/06/school-safe-treats_mini-chocolate-chip-muffins-5.jpg?fit=680%2C800",
        };
        let test_case_section_17 = TestCaseSectionWithImage {
            subtitle: "Fruit Pizza",
            link: "https://pinchofyum.com/fruit-pizza",
            image: "https://pinchofyum.com/wp-content/uploads/Fruit-Pizza-Design-Square.jpg",
        };
        let test_case_section_18 = TestCaseSectionWithImage {
            subtitle: "No-Bake, Nut-Free Powerbites",
            link: "https://lexiscleankitchen.com/nut-free-bites/",
            image: "https://lexiscleankitchen.com/wp-content/uploads/2020/03/Nut-Free-Energy-Balls11.jpg",
        };
        let test_case_section_19 = TestCaseSectionWithImage {
            subtitle: "No-Bake Apple Doughnuts",
            link: "https://www.forkandbeans.com/2017/07/31/no-bake-apple-donuts/",
            image:
                "https://www.forkandbeans.com/wp-content/uploads/2017/07/No-Bake-Apple-Donuts.jpg",
        };
        let test_case_section_20 = TestCaseSectionWithImage {
            subtitle: "Glazed Doughnut Cookies",
            link: "https://www.purewow.com/recipes/glazed-doughnut-cookies-recipe",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/02/doughnut-cookies-fb.jpg",
        };
        let test_case_section_21 = TestCaseSectionWithImage {
            subtitle: "Giant M&M's Cookies",
            link: "https://whatsgabycooking.com/giant-mm-cookies/#recipeJump",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/06/school-safe-treats_mm-cookies.jpg?fit=680%2C800",
        };
        let test_case = TestCaseWithImage {
            url: "https://www.purewow.com/food/nut-free-school-safe-snack-recipes",
            title: "21 School-Safe Treats That Are Allergy- and Kid-Friendly",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/06/school-safe-treats_universal.jpg?fit=1174%2C630",
            sections: vec![
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
                test_case_section_14,
                test_case_section_15,
                test_case_section_16,
                test_case_section_17,
                test_case_section_18,
                test_case_section_19,
                test_case_section_20,
                test_case_section_21,
            ],
        };

        println!("\n🔬 A/B TEST VERSION B: Section-Scoped PureWow Collection");
        println!(
            "Expected: Regression safety test - working collection should maintain performance"
        );

        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_cosmopolitan_collection_version_b_section_scoped() {
        let (fetcher, scraper, extractor) = create_section_scoped_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Drunken Peanut Butter Cups",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a58358/drunken-peanut-butter-cups-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-cocktails-drunken-peanut-butter-cups-1662511251.jpeg?crop=1xw:0.9993201903467029xh;center,top&resize=980:*",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Apple Cider Doughnut Slushie",
            link: "https://www.delish.com/cooking/recipes/a49600/apple-cider-slushies-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/delish-190920-apple-cider-slushies-0178-landscape-pf-1662511511.jpg?crop=0.669xw:1.00xh;0.196xw,0&resize=980:*",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Zombie Brain Shot",
            link: "https://www.tiktok.com/@thespritzeffect/video/7157066556575403306?_r=1&_t=8eFCmAvZUxm",
            image: "https://hips.hearstapps.com/hmg-prod/images/img-7578-jpg-64c0332feefb2.jpg?crop=0.835xw:1.00xh;0.0748xw,0&resize=980:*",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Hocus Pocus Jello Shots",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a55955/hocus-pocus-jell-o-shots-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/1507330734-delish-hocus-pocus-jello-shots-still001-1662510601.jpg?crop=0.378xw:1.00xh;0.269xw,0&resize=980:*",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Nightmare on Bourbon Street",
            link: "https://www.halfbakedharvest.com/nightmare-on-bourbon-street/",
            image: "https://hips.hearstapps.com/hmg-prod/images/nightmare-on-bourbon-street-1-1662509325.jpg?crop=1xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_6 = TestCaseSectionWithImage {
            subtitle: "Apple Cider Mimosas",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a46963/apple-cider-mimosas-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/0bef95d95637d4f6dfc15d7462098c53-1662510108.jpg?crop=0.8333333333333334xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_7 = TestCaseSectionWithImage {
            subtitle: "Grilled Orange Old-Fashioned",
            link: "https://www.countryliving.com/food-drinks/a40993393/grilled-orange-old-fashioned-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/grilled-orange-old-fashioned-1662509858.jpg?crop=0.646xw:0.780xh;0,0.220xh&resize=980:*",
        };
        let test_case_section_8 = TestCaseSectionWithImage {
            subtitle: "Apple Cinnamon Cider Cups",
            link: "https://www.womansday.com/food-recipes/a33807296/apple-cinnamon-cider-cups-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/apple-cinnamon-cider-cups-1662509051.jpg?crop=0.669xw:1.00xh;0.146xw,0&resize=980:*",
        };
        let test_case_section_9 = TestCaseSectionWithImage {
            subtitle: "Witches' Brew Lemonade",
            link: "https://www.delish.com/holiday-recipes/halloween/a29178988/witches-brew-lemonade-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/witches-brew-lemonade-1662508810.jpg?crop=0.596xw:0.897xh;0,0.0348xh&resize=980:*",
        };
        let test_case_section_10 = TestCaseSectionWithImage {
            subtitle: "Creamsicle Punch",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a52743/creamsicle-punch-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/190409-creamsicle-punch-horizontal-1-1662508571.png?crop=0.665798611111111xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_11 = TestCaseSectionWithImage {
            subtitle: "Poison Apple Cocktail",
            link: "https://www.delish.com/cooking/recipe-ideas/a23878264/poison-apple-cocktails-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/d3e14d682f1e7efa5e832c9cad41dfc5-1662508322.jpg?crop=1.00xw:0.947xh;0,0&resize=980:*",
        };
        let test_case_section_12 = TestCaseSectionWithImage {
            subtitle: "Apricot Bourbon Brew",
            link: "https://www.goodhousekeeping.com/food-recipes/a46066/apricot-bourbon-brew-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-party-potion-punch-1662507902.jpg?crop=1xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_13 = TestCaseSectionWithImage {
            subtitle: "Tart Cherry Eyeball Punch",
            link: "https://www.countryliving.com/food-drinks/a36687070/tart-cherry-eyeball-punch/",
            image: "https://hips.hearstapps.com/hmg-prod/images/halloween-party-cocktails-1016-1662504161.jpg?crop=0.835xw:1.00xh;0.0260xw,0&resize=980:*",
        };
        let test_case_section_14 = TestCaseSectionWithImage {
            subtitle: "Cider Sidecar",
            link: "https://www.countryliving.com/food-drinks/a23326064/cider-sidecar-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/cider-sidecar-cl-1018-1662503635.jpg?crop=1xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_15 = TestCaseSectionWithImage {
            subtitle: "Sleepy Hollow Cocktail",
            link: "https://www.halfbakedharvest.com/sleepy-hollow-cocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/sleepy-hollow-cocktail-1-1662503073.jpg?crop=1xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_16 = TestCaseSectionWithImage {
            subtitle: "Cinnamon Apple Margarita",
            link: "https://lalospirits.com/",
            image: "https://hips.hearstapps.com/hmg-prod/images/lalo-cinnamon-apple-marg-2-64c02202db827.jpeg?crop=0.650xw:0.650xh;0.350xw,0.241xh&resize=980:*",
        };
        let test_case_section_17 = TestCaseSectionWithImage {
            subtitle: "Blood Orange Sangria",
            link: "https://www.howsweeteats.com/2013/02/blood-orange-sangria/",
            image: "https://hips.hearstapps.com/hmg-prod/images/2022-09-06-2-1662502497.png?crop=0.321xw:0.722xh;0.184xw,0.114xh&resize=980:*",
        };
        let test_case_section_18 = TestCaseSectionWithImage {
            subtitle: "Black Widow Smash",
            link: "https://www.halfbakedharvest.com/the-black-widow-smash/",
            image: "https://hips.hearstapps.com/hmg-prod/images/2022-09-06-1662501529.png?crop=0.231xw:0.517xh;0.671xw,0.239xh&resize=980:*",
        };
        let test_case_section_19 = TestCaseSectionWithImage {
            subtitle: "Hocus Pocus Punch",
            link: "https://www.howsweeteats.com/2019/10/hocus-pocus-punch-p-s-its-a-mocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/hocus-pocus-punch-3-1662501062.jpg?crop=0.8907892392659897xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_20 = TestCaseSectionWithImage {
            subtitle: "Blood Moon Cocktail",
            link: "https://www.thesexton.com/cocktails/the-sexton-blood-moon/",
            image: "https://www.thesexton.com/wp-content/uploads/2021/11/sexton-bloodmoon.jpg",
        };
        let test_case_section_21 = TestCaseSectionWithImage {
            subtitle: "Pumpkin Cider",
            link: "https://www.newamsterdamvodka.com/",
            image: "https://hips.hearstapps.com/hmg-prod/images/new-amsterdam-pumpkin-cider-1657553140.jpeg?crop=0.8263695450324977xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_22 = TestCaseSectionWithImage {
            subtitle: "The Vampire’s Kiss Cocktail",
            link: "https://www.halfbakedharvest.com/the-vampires-kiss-cocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/vampires-kiss-1628524783.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_23 = TestCaseSectionWithImage {
            subtitle: "Blood Rising Cocktail",
            link: "https://www.halfbakedharvest.com/blood-rising-cocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/blood-rising-1628524830.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_24 = TestCaseSectionWithImage {
            subtitle: "Pumpkin Punch",
            link: "https://www.halfbakedharvest.com/pumpkin-punch/",
            image: "https://hips.hearstapps.com/hmg-prod/images/pumpkin-punch-1628524880.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_25 = TestCaseSectionWithImage {
            subtitle: "Ghost in the Orchard Cocktail",
            link: "https://www.halfbakedharvest.com/ghost-in-the-orchard-cocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/ghost-in-the-orchard-1628524929.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_26 = TestCaseSectionWithImage {
            subtitle: "Mummy White Russian",
            link: "https://www.halfbakedharvest.com/mummy-white-russian/",
            image: "https://hips.hearstapps.com/hmg-prod/images/mummy-white-russian-1628524977.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_27 = TestCaseSectionWithImage {
            subtitle: "Mystic Moon Cocktail",
            link: "https://www.halfbakedharvest.com/mystic-moon-cocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/mystic-moon-1628525021.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_28 = TestCaseSectionWithImage {
            subtitle: "Bourbon Butterbeer",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a56104/spellbound-cocktail-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/spellbound-cocktail-2-1628525073.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_29 = TestCaseSectionWithImage {
            subtitle: "Spellbound Cocktail",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a46964/pomegranate-cider-mimosas-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/gts-spellbound-1628524996.jpeg?crop=0.883xw:1.00xh;0.0510xw,0&resize=980:*",
        };
        let test_case_section_30 = TestCaseSectionWithImage {
            subtitle: "Pomegranate Cider Mimosas",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a54858/spiked-jolly-rancher-punch-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/spiked-jolly-rancher-punch-5-1628525172.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_31 = TestCaseSectionWithImage {
            subtitle: "Spiked Jolly Rancher Punch",
            link: "https://www.delish.com/cooking/recipe-ideas/a26216721/hot-buttered-rum-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/jolly-rancher-halloween-punch-1657829435.jpg?crop=1xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_32 = TestCaseSectionWithImage {
            subtitle: "Blood Wolf Moon",
            link: "https://www.delish.com/cooking/recipe-ideas/a26216721/hot-buttered-rum-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/hot-buttered-rum-2-1628525217.jpeg?crop=1.00xw:1.00xh;0,0&resize=980:*",
        };
        let test_case_section_33 = TestCaseSectionWithImage {
            subtitle: "Haunted Graveyard",
            link: "https://www.sprinklesandsprouts.com/haunted-graveyard-a-halloween-cocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/haunted-graveyard-halloween-cocktail-2-1628525313.jpeg?crop=1xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_34 = TestCaseSectionWithImage {
            subtitle: "Pumpkin Spice White Russian",
            link: "https://www.thecookierookie.com/pumpkin-spice-white-russian-cocktail/",
            image: "https://hips.hearstapps.com/hmg-prod/images/pumpkin-spice-white-russian-6-of-13-1628525386.jpeg?crop=0.9637254901960784xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_35 = TestCaseSectionWithImage {
            subtitle: "Monster Mash Margaritas",
            link: "https://www.freutcake.com/in-the-kitchen/drinks-anyone/monster-mash-margaritas/",
            image: "https://hips.hearstapps.com/hmg-prod/images/monster-mash-cocktail-3-1628525458.jpeg?crop=1.00xw:1.00xh;0.00170xw,0&resize=980:*",
        };
        let test_case_section_36 = TestCaseSectionWithImage {
            subtitle: "Cacao Imperial Old-Fashioned Cocktail",
            link: "https://ronbarcelo.com/en/rum/imperial/",
            image: "https://hips.hearstapps.com/hmg-prod/images/ronbarcelocacaoimperialoldfashionedcocktailtiny-1628525657.png?crop=0.451xw:1.00xh;0.523xw,0&resize=980:*",
        };
        let test_case_section_37 = TestCaseSectionWithImage {
            subtitle: "Blood and Sand Cocktail With Lychee Eyeball",
            link: "https://go.redirectingat.com?id=74968X1525071&url=https%3A%2F%2Fwww.hellofresh.com%2F",
            image: "https://hips.hearstapps.com/hmg-prod/images/hf160928-extrashot-us-halloweentipsheet-42-low-1050x1575-1628527296.jpeg?crop=1xw:1xh;center,top&resize=980:*",
        };
        let test_case_section_38 = TestCaseSectionWithImage {
            subtitle: "Fright White Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_39 = TestCaseSectionWithImage {
            subtitle: "The Apparition Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_40 = TestCaseSectionWithImage {
            subtitle: "Dark and Stormy Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_41 = TestCaseSectionWithImage {
            subtitle: "Absolut Masquerade Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_42 = TestCaseSectionWithImage {
            subtitle: "The Gravedigger Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_43 = TestCaseSectionWithImage {
            subtitle: "Sugarsnake Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_44 = TestCaseSectionWithImage {
            subtitle: "Black Cauldron Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_45 = TestCaseSectionWithImage {
            subtitle: "The Boneyard Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_46 = TestCaseSectionWithImage {
            subtitle: "Heat of the Moment Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_47 = TestCaseSectionWithImage {
            subtitle: "Smoked Pumpkin Cocktail",
            link: "",
            image: "",
        };
        let test_case_section_48 = TestCaseSectionWithImage {
            subtitle: "Midnight’s Shadow Cocktail",
            link: "",
            image: "",
        };
        let test_case = TestCaseWithImage {
            url: "https://www.cosmopolitan.com/food-cocktails/a4896/spooky-halloween-cocktails/",
            title: "48 Spooky Halloween Cocktails to Mix Up for Ghouls Night",
            image: "https://hips.hearstapps.com/hmg-prod/images/48-spooky-halloween-cocktails-to-mix-up-for-ghouls-night-6508b1c4451d9.png?crop=1xw:0.9944392956441149xh;center,top&resize=1200:*",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
                test_case_section_14,
                test_case_section_15,
                test_case_section_16,
                test_case_section_17,
                test_case_section_18,
                test_case_section_19,
                test_case_section_20,
                test_case_section_21,
                test_case_section_22,
                test_case_section_23,
                test_case_section_24,
                test_case_section_25,
                test_case_section_26,
                test_case_section_27,
                test_case_section_28,
                test_case_section_29,
                test_case_section_30,
                test_case_section_31,
                test_case_section_32,
                test_case_section_33,
                test_case_section_34,
                test_case_section_35,
                test_case_section_36,
                test_case_section_37,
                test_case_section_38,
                test_case_section_39,
                test_case_section_40,
                test_case_section_41,
                test_case_section_42,
                test_case_section_43,
                test_case_section_44,
                test_case_section_45,
                test_case_section_46,
                test_case_section_47,
                test_case_section_48,
            ]
            .to_vec(),
        };

        println!("\n🔬 A/B TEST VERSION B: Section-Scoped Cosmopolitan Collection");
        println!("Expected: Large-scale regression test - 48-section collection should maintain performance");

        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_paperlesspost_collection() {
        let (fetcher, scraper, extractor) = create_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Corpse Reviver",
            link: "https://www.liquor.com/recipes/corpse-reviver-no-2/",
            image: "https://www.liquor.com/thmb/OTadfw0Hpd0LAnpbCR7KA1VyJxc=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/corpse-reviver-no2-1200x628-email-0459f27378f04eed99ba99044ca47f99.jpg",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Jekyll & Gin",
            link: "https://www.delish.com/cooking/recipe-ideas/recipes/a44311/jekyll-gin-glowing-cocktails-glow-party-ideas/",
            image: "https://hips.hearstapps.com/del.h-cdn.co/assets/15/42/1024x512/landscape-1444928749-delish-glow-food-jekyll-gin-recipe.jpg?resize=1200:*",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Bloody Mary Syringes",
            link: "https://www.delish.com/cooking/recipe-ideas/a24132876/bloody-mary-syringes-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/bloody-mary-syringes-horizontal2-1540477593.jpg?crop=1.00xw:0.752xh;0,0.118xh&amp;resize=1200:*",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Witches’ Brew Lemonade",
            link: "https://www.delish.com/holiday-recipes/halloween/a29178988/witches-brew-lemonade-recipe/",
            image: "https://hips.hearstapps.com/hmg-prod/images/witches-brew-lemonade-index-66eddb5580cee.jpg?crop=1.00xw:1.00xh;0,0&amp;resize=1200:*",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Zombie’s Shrunken Head",
            link: "https://www.thespruceeats.com/zombie-cocktail-recipe-761643",
            image: "https://www.thespruceeats.com/thmb/oiRY5sLzLk-ytbcZjct6ovpva1g=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/zombie-cocktail-recipe-761643-Hero-5b7424e2c9e77c0050ec7160.jpg",
        };
        let test_case_section_6 = TestCaseSectionWithImage {
            subtitle: "Dracula’s Kiss",
            link: "https://www.thespruceeats.com/draculas-kiss-cherry-vodka-cola-761041",
            image: "https://www.thespruceeats.com/thmb/ufRSYEuPqXj7I18XkRXNawoywYM=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/draculas-kiss-cherry-vodka-cola-761041-hero-01-9ac496ad5fc94f0e920b21ed8d5f46e9.jpg",
        };
        let test_case_section_7 = TestCaseSectionWithImage {
            subtitle: "An American Werewolf in London Fog",
            link: "https://www.thrillist.com/drink/nation/london-fog-cocktail-recipe",
            image: "https://assets3.thrillist.com/v1/image/2748851/1200x600/scale;;webp=auto;jpeg_quality=85.jpg",
        };
        let test_case_section_8 = TestCaseSectionWithImage {
            subtitle: "The Candyman",
            link: "https://cookieandkate.com/bees-knees-cocktail-recipe/",
            image: "https://cookieandkate.com/images/2020/04/bees-knees-drink.jpg",
        };
        let test_case_section_9 = TestCaseSectionWithImage {
            subtitle: "The Walking Dead",
            link: "https://craftandcocktails.co/2015/10/31/walking-dead-a-halloween-cocktail/",
            image: "https://craftandcocktails.co/wp-content/uploads/2015/10/walking-dead-cocktail-3.jpg",
        };
        let test_case_section_10 = TestCaseSectionWithImage {
            subtitle: "Slimer’s Ectoplasm",
            link: "https://diycandy.com/ghostbusters-cocktail/",
            image: "https://diycandy.b-cdn.net/wp-content/uploads/2016/08/Slimer-Ectoplasm-slime-cocktail-e1470531855283.jpg",
        };
        let test_case_section_11 = TestCaseSectionWithImage {
            subtitle: "Bourbon Butterbeer",
            link: "https://www.gastronomblog.com/bourbon-butterbeer/",
            image:
                "https://www.gastronomblog.com/wp-content/uploads/2016/10/bourbonbutterbeer-6.jpg",
        };
        let test_case_section_12 = TestCaseSectionWithImage {
            subtitle: "Polyjuice Potion",
            link: "https://www.crowdedkitchen.com/polyjuice-potion-cocktail/",
            image: "https://www.crowdedkitchen.com/wp-content/uploads/2020/10/potion.jpg",
        };
        let test_case_section_13 = TestCaseSectionWithImage {
            subtitle: "Rosemary’s Baby Punch",
            link: "https://www.seriouseats.com/prime-meats-whiskey-aperol-punch-rosemary-baby",
            image: "https://www.seriouseats.com/thmb/IFtiGfNr1PE6gZV2kxkOggNrqlk=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/__opt__aboutcom__coeus__resources__content_migration__serious_eats__seriouseats.com__recipes__images__20111114-PrimeMeats-1-9b86b6f67a6a4914ab4c48d27e3331d1.jpg",
        };
        let test_case_section_14 = TestCaseSectionWithImage {
            subtitle: "Demagorgon’s Dinner",
            link: "https://www.mainespirits.com/recipes/demogorgons-dinner",
            image: "https://www.mainespirits.com/recipes/demogorgons-dinner",
        };
        let test_case_section_15 = TestCaseSectionWithImage {
            subtitle: "The Silver Bullet",
            link: "https://www.vice.com/en/article/this-cocktail-is-potent-enough-to-kill-a-werewolf-and-its-made-with-real-silver/",
            image: "https://munchies-images.vice.com/wp_upload/silver-bullet.jpg?resize=1000:*",
        };
        let test_case_section_16 = TestCaseSectionWithImage {
            subtitle: "Michael Meyer’s Lemon Drop",
            link: "https://www.iconiccocktail.com/products/meyer-lemon-drop",
            image: "http://www.iconiccocktail.com/cdn/shop/products/Iconic_Meyer_Lemon_Balm_10_of_15_dcaa2ba9-323b-49f4-8d95-74b0f2ef1750.jpg?v=1559849900",
        };
        let test_case_section_17 = TestCaseSectionWithImage {
            subtitle: "Manhattan Chainsaw Massacre",
            link: "https://www.seriouseats.com/cocktails-manhattan-recipe",
            image: "https://www.seriouseats.com/thmb/ADgVAVIRqXTxPlSnX9Xjntiv7m8=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/20230811-SEA-Manhattan-TwoBites-005-8e3a7657e623426d9625a25fb362bcd1.jpg",
        };
        let test_case_section_18 = TestCaseSectionWithImage {
            subtitle: "Cthulhu’s Mai-Tai",
            link: "https://www.epicurious.com/recipes/food/views/mai-tai-230577",
            image: "https://assets.epicurious.com/photos/6239dd8cfc699f0e516897df/16:9/w_1280,c_limit/MaiTai_RECIPE_031722_30061.jpg",
        };
        let test_case_section_19 = TestCaseSectionWithImage {
            subtitle: "The Love Witch’s Earl Grey Martini",
            link: "https://www.allrecipes.com/recipe/162319/earl-grey-martini/",
            image: "https://www.allrecipes.com/thmb/vbYUZSg1lEZjdkbjMZfgqefMlko=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/875004-97005ad8419545dca0c2f248864fdb4d.jpg",
        };
        let test_case_section_20 = TestCaseSectionWithImage {
            subtitle: "Freddy’s Dream Warriors",
            link: "https://everydayshortcuts.com/freddy-krueger-dream-warriors-cocktail/",
            image: "https://everydayshortcuts.com/wp-content/uploads/2022/07/freddy-kreuger-dream-warriors-cocktail.jpg",
        };
        let test_case = TestCaseWithImage {
            url: "https://www.paperlesspost.com/blog/halloween-cocktails-drinks/",
            title: "20 Spooky Halloween cocktails and drinks",
            image: "https://www.paperlesspost.com/blog/wp-content/uploads/Opt2_092622_Blog_HalloweenCocktails_01-hero.png",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
                test_case_section_14,
                test_case_section_15,
                test_case_section_16,
                test_case_section_17,
                test_case_section_18,
                test_case_section_19,
                test_case_section_20,
            ].to_vec(),
        };
        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_101cookbooks_collection() {
        let (fetcher, scraper, extractor) = create_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Cardinale",
            link: "https://punchdrink.com/recipes/cardinale/",
            image: "https://punchdrink.com/wp-content/uploads/2015/01/Social-Cardinale.jpg",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Blood Orange Test Tubes",
            link: "https://www.marthastewart.com/852648/blood-orange-cocktails",
            image: "https://www.marthastewart.com/thmb/rdy9DS-Ib96QpE8AT-A1Nfc3lVk=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/MS-332441-mulled-wine-hero-7211-541c12ab7a0347f59d9f6bcd7044d0d9.jpg",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Jekyll Gin Glowing Cocktails",
            link: "http://www.delish.com/cooking/recipe-ideas/recipes/a44311/jekyll-gin-glowing-cocktails-glow-party-ideas/",
            image: "https://hips.hearstapps.com/del.h-cdn.co/assets/15/42/1024x512/landscape-1444928749-delish-glow-food-jekyll-gin-recipe.jpg?resize=1200:*",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Pirate Mary",
            link: "http://www.foodandwine.com/recipes/pirate-mary",
            image: "https://www.foodandwine.com/thmb/j5nqXBcGiYAv5odTSNLs1HOi1MA=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/Batch-Cocktails-That-You-Can-Prep-in-Advance-FT-BLOG1023-02ccb07cf13241ec9acd30779c81a696.jpg",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Kombucha Dark & Stormy",
            link: "https://www.101cookbooks.com/archives/kombucha-dark-and-stormy-recipe.html",
            image: "https://images.101cookbooks.com/kombucha-dark-and-stormy-h.jpg?w=680",
        };
        let test_case_section_6 = TestCaseSectionWithImage {
            subtitle: "Death in the Afternoon",
            link: "https://punchdrink.com/recipes/death-in-the-afternoon/",
            image: "https://punchdrink.com/wp-content/uploads/2013/09/Death-Afternoon.jpg",
        };
        let test_case_section_7 = TestCaseSectionWithImage {
            subtitle: "Mother's Ruin Punch",
            link: "http://www.foodandwine.com/recipes/mothers-ruin-punch",
            image: "https://www.foodandwine.com/thmb/8Y6ZLwAujXWDvRHjgjXiVJ_j_HA=/1500x0/filters:no_upscale():max_bytes(150000):strip_icc()/Mothers-Ruin-Punch-FT-RECIPE1023-db7969e34072469b98c09ba57410b753.jpg",
        };
        let test_case = TestCaseWithImage {
            url: "https://www.101cookbooks.com/7-halloween-cocktails/",
            title: "7 Halloween Cocktails You’re Less Likely to Regret",
            image: "https://images.101cookbooks.com/halloween-cocktails-h.jpg?w=680",
            sections: [
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
            ]
            .to_vec(),
        };
        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_tasting_table_recipe() {
        let (fetcher, scraper, extractor) = create_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case = TestCaseWithImage {
            url: "https://www.tastingtable.com/1416554/vampires-kiss-halloween-cocktail-recipe/",
            title: "Vampire's Kiss Halloween Cocktail Recipe",
            image: "https://www.tastingtable.com/img/gallery/vampires-kiss-halloween-cocktail-recipe/l-intro-1696949215.jpg",
            sections: vec![],
        };
        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_foodnetwork_cake_recipe() {
        let (fetcher, scraper, extractor) = create_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case = TestCaseWithImage {
            url: "https://www.foodnetwork.com/recipes/southern-red-velvet-cake-recipe-2011892",
            title: "Southern Red Velvet Cake",
            image: "https://food.fnr.sndimg.com/content/dam/images/food/fullset/2004/1/23/1/ss1d26_red_velvet_cake.jpg.rend.hgtvcom.616.462.suffix/1371584132020.webp",
            sections: vec![],
        };
        test_url_extraction_with_image(&test_case, &engine);
    }

    #[test]
    fn test_purewow_collection() {
        let (fetcher, scraper, extractor) = create_test_engine();
        let engine = Engine {
            fetcher: &fetcher,
            scraper: &scraper,
            extractor: &extractor,
            opts: EngineOptions { max_children: 0 },
        };
        let test_case_section_1 = TestCaseSectionWithImage {
            subtitle: "Chewy Chocolate Chip Granola Bars",
            link: "https://www.purewow.com/recipes/chewy-chocolate-chip-granola-bars",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2022/08/chewy-chocolate-chip-granola-bars-recipe-FB.jpg",
        };
        let test_case_section_2 = TestCaseSectionWithImage {
            subtitle: "Gluten-Free Flourless Cocoa Cookies",
            link: "https://www.purewow.com/recipes/gluten-free-flourless-cookies",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2017/10/flourless-chocolate-cookies-fb.jpg",
        };
        let test_case_section_3 = TestCaseSectionWithImage {
            subtitle: "Homemade Cinnamon Applesauce",
            link: "https://www.purewow.com/recipes/homemade-cinnamon-applesauce-recipe",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2015/12/applesauce-400.jpg?fit=400%2C290",
        };
        let test_case_section_4 = TestCaseSectionWithImage {
            subtitle: "Chocolate Chip Cookie Dough Dip",
            link: "https://www.purewow.com/recipes/chocolate-chip-cookie-dough-dip",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2016/10/cookiedip-400.png?fit=400%2C290",
        };
        let test_case_section_5 = TestCaseSectionWithImage {
            subtitle: "Cookies-and-Cream Ice Pops",
            link: "https://www.purewow.com/recipes/cookies-and-cream-ice-pops-recipe",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2017/07/cookies-and-cream-pops-630-fb.jpg",
        };
        let test_case_section_6 = TestCaseSectionWithImage {
            subtitle: "Breakfast Hand Pies",
            link: "https://www.purewow.com/recipes/breakfast-hand-pies",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2014/03/poptartfb.jpg",
        };
        let test_case_section_7 = TestCaseSectionWithImage {
            subtitle: "Strawberry Oatmeal Bars",
            link: "https://www.wellplated.com/strawberry-oatmeal-bars/",
            image: "https://www.wellplated.com/wp-content/uploads/2016/03/Easy-Strawberry-Oatmeal-Bars.jpg",
        };
        let test_case_section_8 = TestCaseSectionWithImage {
            subtitle: "Mini Caramel Apples",
            link: "https://www.purewow.com/recipes/mini-caramel-apples",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/06/school-safe-treats_mini-caramel-apples.jpeg?fit=680%2C860",
        };
        let test_case_section_9 = TestCaseSectionWithImage {
            subtitle: "Vegan and Gluten-Free Baked Doughnuts",
            link: "https://www.purewow.com/recipes/baked-gluten-free-doughnuts",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2021/02/vegan-gluten-free-baked-doughnuts-recipe-fb.jpg",
        };
        let test_case_section_10 = TestCaseSectionWithImage {
            subtitle: "Magic Pancakes with Bananas, Eggs and Yogurt",
            link: "https://www.purewow.com/recipes/magic-pancakes-with-bananas-eggs-and-yogurt",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2022/04/daphne-oz-magic-pancakes-recipe-fb.jpg",
        };
        let test_case_section_11 = TestCaseSectionWithImage {
            subtitle: "Apple Cider Doughnut Holes",
            link: "https://www.purewow.com/recipes/apple-cider-doughnut-holes",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/06/school-safe-treats_apple-cider-doughnut-holes.jpeg?fit=680%2C860",
        };
        let test_case_section_12 = TestCaseSectionWithImage {
            subtitle: "Watercolor Doughnuts",
            link: "https://www.purewow.com/recipes/watercolor-doughnuts",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/05/watercolor-doughnuts-recipe-fb.jpg",
        };
        let test_case_section_13 = TestCaseSectionWithImage {
            subtitle: "5-Ingredient Frozen Yogurt Bites",
            link: "https://playswellwithbutter.com/5-ingredient-frozen-yogurt-bites/",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/06/school-safe-treats_FROZEN-YOGURT-BITES-1.jpg?fit=680%2C800",
        };
        let test_case_section_14 = TestCaseSectionWithImage {
            subtitle: "Snickerdoodle Lucky Charms Cookies",
            link: "https://iamafoodblog.com/snickerdoodle-lucky-charms-cookies/",
            image: "https://iamafoodblog.b-cdn.net/wp-content/uploads/2019/03/lucky-charms-snickerdoodles-8702w.jpg'",
        };
        let test_case_section_15 = TestCaseSectionWithImage {
            subtitle: "Silly Apple Bites",
            link: "https://www.forkandbeans.com/2015/08/06/silly-apple-bites/",
            image: "https://www.forkandbeans.com/wp-content/uploads/2015/08/Silly-Apple-Bites.jpg",
        };
        let test_case_section_16 = TestCaseSectionWithImage {
            subtitle: "Mini Chocolate Chip Muffins",
            link: "https://eatwithclarity.com/mini-chocolate-chip-muffins/",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/06/school-safe-treats_mini-chocolate-chip-muffins-5.jpg?fit=680%2C800",
        };
        let test_case_section_17 = TestCaseSectionWithImage {
            subtitle: "Fruit Pizza",
            link: "https://pinchofyum.com/fruit-pizza",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/06/school-safe-treats_Fruit-Pizza.jpg?fit=680%2C800",
        };
        let test_case_section_18 = TestCaseSectionWithImage {
            subtitle: "No-Bake, Nut-Free Powerbites",
            link: "https://lexiscleankitchen.com/nut-free-bites/",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/06/school-safe-treats_Nut-Free-Energy-Balls.jpg?fit=680%2C800",
        };
        let test_case_section_19 = TestCaseSectionWithImage {
            subtitle: "No-Bake Apple Doughnuts",
            link: "https://www.forkandbeans.com/2017/07/31/no-bake-apple-donuts/",
            image:
                "https://www.forkandbeans.com/wp-content/uploads/2017/07/No-Bake-Apple-Donuts.jpg",
        };
        let test_case_section_20 = TestCaseSectionWithImage {
            subtitle: "Glazed Doughnut Cookies",
            link: "https://www.purewow.com/recipes/glazed-doughnut-cookies-recipe",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/02/doughnut-cookies-fb.jpg",
        };
        let test_case_section_21 = TestCaseSectionWithImage {
            subtitle: "Giant M&M's Cookies",
            link: "https://whatsgabycooking.com/giant-mm-cookies/#recipeJump",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/06/school-safe-treats_mm-cookies.jpg?fit=680%2C800",
        };
        let test_case = TestCaseWithImage {
            url: "https://www.purewow.com/food/nut-free-school-safe-snack-recipes",
            title: "21 School-Safe Treats That Are Allergy- and Kid-Friendly",
            image: "https://publish.purewow.net/wp-content/uploads/sites/2/2018/06/school-safe-treats_universal.jpg?fit=1174%2C630",
            sections: vec![
                test_case_section_1,
                test_case_section_2,
                test_case_section_3,
                test_case_section_4,
                test_case_section_5,
                test_case_section_6,
                test_case_section_7,
                test_case_section_8,
                test_case_section_9,
                test_case_section_10,
                test_case_section_11,
                test_case_section_12,
                test_case_section_13,
                test_case_section_14,
                test_case_section_15,
                test_case_section_16,
                test_case_section_17,
                test_case_section_18,
                test_case_section_19,
                test_case_section_20,
                test_case_section_21,
            ],
        };
        test_url_extraction_with_image(&test_case, &engine);
    }

    // Google Site Search Tests for Good Housekeeping sections 21-24

    #[tokio::test]
    async fn test_google_site_search_shirley_temple() {
        use crate::services::GoogleSiteSearch;

        let searcher = GoogleSiteSearch::new().unwrap();
        let domain = "www.goodhousekeeping.com";
        let subtitle = "Shirley Temple Drink";

        let result = searcher.search_site_for_subtitle(domain, subtitle).await;

        // Should find a URL containing "shirley-temple" or similar
        if let Some(url) = result {
            println!("Found URL for Shirley Temple: {}", url);
            assert!(url.contains("goodhousekeeping.com"));
            // Expect to find recipe-related content
            assert!(
                url.to_lowercase().contains("shirley")
                    || url.to_lowercase().contains("temple")
                    || url.to_lowercase().contains("drink")
                    || url.to_lowercase().contains("recipe")
            );
        } else {
            // Google may block automated requests, so we'll skip this test for now
            println!("Warning: No URL found for Shirley Temple Drink (Google may be blocking automated requests)");
        }
    }

    #[tokio::test]
    async fn test_google_site_search_ginger_sangria() {
        use crate::services::GoogleSiteSearch;

        let searcher = GoogleSiteSearch::new().unwrap();
        let domain = "www.goodhousekeeping.com";
        let subtitle = "Sparkling Ginger Sangria";

        let result = searcher.search_site_for_subtitle(domain, subtitle).await;

        // Should find a URL containing "ginger", "sangria" or similar
        if let Some(url) = result {
            println!("Found URL for Sparkling Ginger Sangria: {}", url);
            assert!(url.contains("goodhousekeeping.com"));
            // Expect to find recipe-related content
            assert!(
                url.to_lowercase().contains("ginger")
                    || url.to_lowercase().contains("sangria")
                    || url.to_lowercase().contains("sparkling")
                    || url.to_lowercase().contains("recipe")
            );
        } else {
            // Google may block automated requests, so we'll skip this test for now
            println!("Warning: No URL found for Sparkling Ginger Sangria (Google may be blocking automated requests)");
        }
    }

    #[tokio::test]
    async fn test_google_site_search_cherry_sidecar() {
        use crate::services::GoogleSiteSearch;

        let searcher = GoogleSiteSearch::new().unwrap();
        let domain = "www.goodhousekeeping.com";
        let subtitle = "Cherry Sidecar";

        let result = searcher.search_site_for_subtitle(domain, subtitle).await;

        // Should find a URL containing "cherry", "sidecar" or similar
        if let Some(url) = result {
            println!("Found URL for Cherry Sidecar: {}", url);
            assert!(url.contains("goodhousekeeping.com"));
            // Expect to find recipe-related content
            assert!(
                url.to_lowercase().contains("cherry")
                    || url.to_lowercase().contains("sidecar")
                    || url.to_lowercase().contains("cocktail")
                    || url.to_lowercase().contains("recipe")
            );
        } else {
            // Google may block automated requests, so we'll skip this test for now
            println!("Warning: No URL found for Cherry Sidecar (Google may be blocking automated requests)");
        }
    }

    #[tokio::test]
    async fn test_google_site_search_charcoal_lemonade() {
        use crate::services::GoogleSiteSearch;

        let searcher = GoogleSiteSearch::new().unwrap();
        let domain = "www.goodhousekeeping.com";
        let subtitle = "Black Charcoal Lemonade Halloween Cocktail";

        let result = searcher.search_site_for_subtitle(domain, subtitle).await;

        // Should find a URL containing "charcoal", "lemonade" or similar
        if let Some(url) = result {
            println!("Found URL for Black Charcoal Lemonade: {}", url);
            assert!(url.contains("goodhousekeeping.com"));
            // Expect to find recipe-related content
            assert!(
                url.to_lowercase().contains("charcoal")
                    || url.to_lowercase().contains("lemonade")
                    || url.to_lowercase().contains("black")
                    || url.to_lowercase().contains("halloween")
                    || url.to_lowercase().contains("recipe")
            );
        } else {
            // Google may block automated requests, so we'll skip this test for now
            println!("Warning: No URL found for Black Charcoal Lemonade Halloween Cocktail (Google may be blocking automated requests)");
        }
    }
}
