use sounda::{
    browser::{Browser, Entry},
    keyboard::Keyboard,
    playlist::Mode,
    ui::{self, NowPlaying, PIXELS, Pane},
};
use std::{fs, io::Write};

fn main() {
    let mut b = Browser::new();
    b.replace(
        "sd:/Music/Night drive".into(),
        vec![
            Entry {
                name: "Albums".into(),
                directory: true,
            },
            Entry {
                name: "01 - Into the blue.m4a".into(),
                directory: false,
            },
            Entry {
                name: "02 - City lights.m4a".into(),
                directory: false,
            },
            Entry {
                name: "03 - After midnight.m4a".into(),
                directory: false,
            },
            Entry {
                name: "04 - A quiet place.m4a".into(),
                directory: false,
            },
            Entry {
                name: "05 - Home again.m4a".into(),
                directory: false,
            },
        ],
        false,
    );
    b.selected = 2;
    let n = NowPlaying {
        title: "02 - City lights",
        folder: "Night drive",
        status: "NOW PLAYING",
        elapsed: 83,
        duration: 246,
        paused: false,
        loaded: true,
        mode: Mode::Sequential,
        error: None,
        scanning: false,
        sequence: Some((2, 5)),
        position: (83 * u16::MAX as u32 / 246) as u16,
        clock: 12 * 3600 + 34 * 60 + 56,
        ..NowPlaying::default()
    };
    fs::create_dir_all("output").unwrap();
    let mut top = vec![0; PIXELS];
    let mut bottom = vec![0; PIXELS];
    ui::render(&mut top, &mut bottom, &b, &n);
    save("output/preview.ppm", &top, &bottom);
    let mut expanded = Browser::new();
    expanded.replace(
        "sd:/Music / 音楽".into(),
        (0..200)
            .map(|i| Entry {
                name: format!(
                    "{i:03} {}.m4a",
                    [
                        "Beyoncé - Déjà vu",
                        "日本語の歌",
                        "中国音乐",
                        "Привет мир",
                        "Zażółć gęślą jaźń"
                    ][i % 5]
                ),
                directory: false,
            })
            .collect(),
        false,
    );
    expanded.set_visible_rows(ui::view_rows(Pane::Library, true));
    expanded.move_by(9);
    let expanded_now = NowPlaying { listing: true, ..n };
    ui::render(&mut top, &mut bottom, &expanded, &expanded_now);
    save("output/preview-browse.ppm", &top, &bottom);
    let mut multilingual = Browser::new();
    multilingual.replace(
        "sd:/音楽 / Música".into(),
        [
            "01 Beyoncé - Déjà vu.m4a",
            "02 Zażółć gęślą jaźń.m4a",
            "03 日本語の歌.m4a",
            "04 中国音乐.m4a",
            "05 Привет мир.m4a",
        ]
        .iter()
        .map(|name| Entry {
            name: (*name).into(),
            directory: false,
        })
        .collect(),
        false,
    );
    let mut unicode_now = NowPlaying {
        title: "Привет мир - Ночная музыка",
        folder: "音楽 / Música",
        queue_len: 5,
        ..n
    };
    ui::render(&mut top, &mut bottom, &multilingual, &unicode_now);
    save("output/preview-unicode.ppm", &top, &bottom);
    unicode_now.pane = Pane::Search;
    unicode_now.listing = true;
    multilingual.set_visible_rows(ui::view_rows(Pane::Search, true));
    multilingual.set_query("beyonce".into());
    while multilingual.searching() {
        multilingual.search_step();
    }
    let keys = Keyboard::Latin.keys(&[]);
    unicode_now.keyboard_keys = &keys[..30];
    ui::render(&mut top, &mut bottom, &multilingual, &unicode_now);
    save("output/preview-search.ppm", &top, &bottom);
    unicode_now.pane = Pane::Queue;
    multilingual.replace_ordered(
        "UP NEXT / 3 SONGS".into(),
        [
            "01 中国音乐.m4a",
            "02 Beyoncé - Déjà vu.m4a",
            "03 日本語の歌.m4a",
        ]
        .iter()
        .map(|name| Entry {
            name: (*name).into(),
            directory: false,
        })
        .collect(),
        false,
    );
    multilingual.set_visible_rows(ui::view_rows(Pane::Queue, true));
    multilingual.selected = 1;
    ui::render(&mut top, &mut bottom, &multilingual, &unicode_now);
    save("output/preview-queue.ppm", &top, &bottom);
    let n = NowPlaying {
        error: Some("Use AAC-LC audio (HE-AAC/ALAC unsupported)"),
        paused: true,
        ..n
    };
    ui::render(&mut top, &mut bottom, &b, &n);
    save("output/preview-error.ppm", &top, &bottom);
    println!("output/preview.ppm\noutput/preview-error.ppm");
}
fn save(path: &str, top: &[u16], bottom: &[u16]) {
    let mut f = fs::File::create(path).unwrap();
    writeln!(f, "P6\n256 392\n255").unwrap();
    for screen in [top, bottom] {
        if core::ptr::eq(screen, bottom) {
            f.write_all(&[0; 256 * 8 * 3]).unwrap();
        }
        for &pixel in screen {
            let colors = [pixel & 31, (pixel >> 5) & 31, (pixel >> 10) & 31];
            f.write_all(&colors.map(|c| ((c * 255 + 15) / 31) as u8))
                .unwrap();
        }
    }
}
