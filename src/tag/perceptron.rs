//! An averaged perceptron part of speech tagger
//! Code adapted from [NLTK](http://www.nltk.org/_modules/nltk/tag/perceptron.html) and
//! [prose](https://github.com/jdkato/prose/blob/master/tag/aptag.go).
//!
//! Based on [an algorithm by Matthew Honnibal](https://github.com/jdkato/prose/blob/master/tag/aptag.go)

use error::*;
use super::*;

use bincode::{deserialize, serialize, Infinite};
use failure::ResultExt;
use itertools::Itertools;
use rand::{thread_rng, Rng};
use std::cmp::min;
use std::collections::{HashMap, HashSet};
use std::io::prelude::*;
use std::fs::File;
use std::path::Path;

#[derive(Clone, PartialEq, Debug, Default, Deserialize, Serialize)]
pub struct AveragedPerceptron {
    classes: HashSet<String>,
    instances: usize,
    stamps: HashMap<String, f64>,
    totals: HashMap<String, f64>,
    weights: HashMap<String, HashMap<String, f64>>,
}

impl AveragedPerceptron {
    pub fn new() -> AveragedPerceptron {
        AveragedPerceptron::default()
    }

    pub fn weights(mut self, weights: HashMap<String, HashMap<String, f64>>) -> AveragedPerceptron {
        self.weights = weights;
        self
    }

    pub fn classes(mut self, classes: HashSet<String>) -> AveragedPerceptron {
        self.classes = classes;
        self
    }

    pub fn predict(&self, features: &HashMap<String, f64>) -> Result<String, SmolError> {
        let mut scores: HashMap<String, f64> = HashMap::new();
        for (feat, val) in features {
            if self.weights.get(feat).is_none() || *val == 0.0 {
                continue;
            }
            let weights = &self.weights[feat];
            for (label, weight) in weights {
                scores
                    .get_mut(label)
                    .map(|w| *w += (*weight as f64 * val) as f64)
                    .unwrap_or_else(|| {
                        scores.insert(label.to_owned(), (*weight as f64 * val) as f64);
                    });
            }
        }

        scores
            .iter()
            .map(|i| ((i.1 * 100000.0) as isize, i.0))
            .max()
            .map(|x| x.1.clone())
            .ok_or_else(|| SmolErrorKind::EmptyModel.into())
    }

    pub fn update(&mut self, truth: &str, guess: &str, features: &HashMap<String, f64>) {
        self.instances += 1;
        if truth == guess {
            return;
        }

        for f in features.keys() {
            self.weights
                .get(f)
                .and_then(|weights| {
                    Some((
                        *weights.get(truth).unwrap_or(&0.0),
                        *weights.get(guess).unwrap_or(&0.0),
                    ))
                })
                .and_then(|weights| {
                    self.update_feat(truth, f.as_ref(), weights.0, 1.0);
                    self.update_feat(guess, f.as_ref(), weights.1, -1.0);
                    Some(())
                })
                .or_else(|| {
                    self.update_feat(truth, f.as_ref(), 0.0, 1.0);
                    self.update_feat(guess, f.as_ref(), 0.0, -1.0);
                    Some(())
                });
        }
    }

    pub fn average_weights(&mut self) {
        let stamps = &mut self.stamps;
        for (feat, weights) in &mut self.weights {
            let mut new: HashMap<String, f64> = HashMap::new();
            for (class, weight) in weights.clone() {
                let key = format!("{}-{}", feat, class);
                let delta = stamps
                    .get_mut(&key)
                    .and_then(|v| Some(*v))
                    .or_else(|| {
                        let v = stamps.insert(key.to_owned(), 0.0).unwrap();
                        Some(v)
                    })
                    .unwrap();

                let total = self.totals.entry(key).or_insert(0.0);
                *total += (self.instances as f64 - delta) * weight;
                let averaged = (*total / (self.instances as f64) * 1000.0).round() / 1000.0;
                if averaged != 0.0 {
                    new.insert(class.to_string(), averaged);
                }
            }
            *weights = new;
        }
    }

    fn update_feat(&mut self, c: &str, f: &str, v: f64, w: f64) {
        let key = format!("{}-{}", c, f);

        // TODO: Right now we're accessing the HashMap twice for everything so that we don't have
        // to constantly copy strings
        // Maybe there's a better way...
        self.totals
            .get_mut(&key)
            .and_then(|_| Some(()))
            .or_else(|| {
                self.totals.insert(key.to_owned(), 0.0);
                Some(())
            });

        let delta = self.stamps
            .get_mut(&key)
            .and_then(|v| Some(*v))
            .or_else(|| {
                let v = self.stamps.insert(key.to_owned(), 0.0).unwrap();
                Some(v)
            })
            .unwrap();

        *self.totals.get_mut(&key).unwrap() += (self.instances as f64 - delta) * w;
        *self.stamps.get_mut(&key).unwrap() = self.instances as f64;
        self.weights
            .entry(key)
            .or_insert_with(HashMap::new)
            .get_mut(c)
            .and_then(|val| {
                *val = w + v;
                Some(())
            })
            .or_else(|| Some(()));
    }
}

#[derive(Clone, PartialEq, Debug, Default, Deserialize, Serialize)]
pub struct PerceptronTagger {
    model: AveragedPerceptron,
    tags: HashMap<String, String>,
}

impl PerceptronTagger {
    pub fn new() -> PerceptronTagger {
        PerceptronTagger::default()
    }

    pub fn save(&self, path: &str) -> Result<(), SmolError> {
        let s = serialize(
            &(&self.model.weights, &self.tags, &self.model.classes),
            Infinite,
        ).context(SmolErrorKind::Serialize)?;

        let p = Path::new(path);
        let mut f = File::create(p).context(SmolErrorKind::Write)?;

        f.write_all(&s).context(SmolErrorKind::Write)?;

        Ok(())
    }

    pub fn load(path: &str) -> Result<PerceptronTagger, SmolError> {
        let p = Path::new(path);
        let mut f = File::open(p).context(SmolErrorKind::Write)?;

        let mut s = String::new();
        f.read_to_string(&mut s).context(SmolErrorKind::Write)?;
        let (weights, tags, classes) =
            deserialize(s.as_bytes()).context(SmolErrorKind::Deserialize)?;

        let m = AveragedPerceptron::new().weights(weights).classes(classes);

        let p = PerceptronTagger {
            model: m,
            tags: tags,
        };

        Ok(p)
    }

    pub fn pos<'a, I: IntoIterator<Item = Token<'a>>>(
        &mut self,
        words: I,
    ) -> Result<Vec<(Token<'a>, String)>, SmolError> {
        let clean = words.into_iter();

        let mut context = vec![
            "-START-".to_owned(),
            "-START2-".to_owned(),
            "-END-".to_owned(),
            "-END2-".to_owned(),
        ];
        let mut c = Vec::new();
        let mut ix = 2;

        for i in clean {
            context.insert(ix, Self::normalize_str(&i.term));
            c.push(i);
            ix += 1;
        }

        let (mut p1, mut p2) = ("-START-".to_owned(), "-START2-".to_owned());

        let mut res = Vec::with_capacity(c.len());

        for (i, word) in c.into_iter().enumerate() {
            let tag = match self.tags.get(&*word.term) {
                Some(s) => s.to_string(),
                None => {
                    let features = Self::get_features(i, &context[..], &*word.term, &p1, &p2);
                    self.model.predict(&features)?
                }
            };

            if &*word.term != "-START-" || &*word.term != "-START2-" || &*word.term != "-END-"
                || &*word.term != "-END2-"
            {
                res.push((word, tag.to_owned()));
            }

            p2 = p1;
            p1 = tag;
        }

        Ok(res)
    }

    // TODO: How to ensure we have sentences
    pub fn train(&mut self, mut sentences: Vec<TaggedSentence>, iterations: usize) {
        self.make_tags(&sentences);
        for _ in 0..iterations {
            for sentence in &mut sentences {
                let (words, tags): (Vec<_>, Vec<_>) = sentence.iter().cloned().unzip();

                let context = vec!["-START-".to_owned(), "-START2-".to_owned()]
                    .into_iter()
                    .chain(words.iter().map(|x| Self::normalize_str(x)))
                    .chain(vec!["-END-".to_owned(), "-END2-".to_owned()].into_iter())
                    .collect::<Vec<_>>();

                let (mut p1, mut p2) = ("-START-".to_owned(), "-START2-".to_owned());

                for (i, word) in words.iter().enumerate() {
                    let guess = match self.tags.get(word) {
                        Some(s) => s.to_owned(),
                        None => {
                            let features = Self::get_features(i, &context[..], word, &p1, &p2);
                            let g = self.model.predict(&features).unwrap();
                            self.model.update(&tags[i], &g, &features);
                            g
                        }
                    };

                    p2 = p1;
                    p1 = guess;
                }
            }
            let mut rng = thread_rng();
            rng.shuffle(&mut sentences);
        }
        self.model.average_weights();
    }

    // TODO: How to ensure we have sentences
    fn make_tags(&mut self, sentences: &[TaggedSentence]) {
        let mut counts: HashMap<&str, HashMap<&str, usize>> = HashMap::new();
        for sentence in sentences {
            for &(ref word, ref tag) in *sentence {
                let hm = counts.entry(word).or_insert_with(HashMap::new);
                *hm.entry(tag).or_insert(0) += 1;
                self.model.classes.insert(tag.to_string());
            }
        }
        for (word, tag_freq) in counts {
            let (tag, mode) = tag_freq.iter().max().unwrap();
            let n = tag_freq.iter().map(|x| x.1).fold(0, |acc, &x| acc + x) as f64;

            let freq_thresh = 20.0;
            let ambiguity_thresh = 0.97;

            if n >= freq_thresh && (*mode as f64 / n) >= ambiguity_thresh {
                self.tags.insert(word.to_string(), tag.to_string());
            }
        }
    }

    fn get_features(
        i: usize,
        context: &[String],
        w: &str,
        p1: &str,
        p2: &str,
    ) -> HashMap<String, f64> {
        let w = w.chars().collect::<Vec<_>>();
        let suf = min(w.len(), 3);
        let i = min(context.len() - 2, i + 2);
        let iminus = min(context[i - 1].len(), 3);
        let iplus = min(context[i + 1].len(), 3);

        let mut res = HashMap::new();
        Self::add_feature(&["bias"], &mut res);
        Self::add_feature(
            &["i suffix", &w[w.len() - suf..].iter().collect::<String>()],
            &mut res,
        );
        Self::add_feature(&["i pref1", &w[0].to_string()], &mut res);
        Self::add_feature(&["i-1 tag", p1], &mut res);
        Self::add_feature(&["i-2 tag", p2], &mut res);
        Self::add_feature(&["i tag+i-2 tag", p1, p2], &mut res);
        Self::add_feature(&["i word", &context[i]], &mut res);
        Self::add_feature(&["i-1 tag+i word", p1, &context[i]], &mut res);
        Self::add_feature(&["i-1 word", &context[i - 1]], &mut res);
        Self::add_feature(
            &[
                "i-1 suffix",
                &context[i - 1][context[i - 1].len() - iminus..],
            ],
            &mut res,
        );
        Self::add_feature(&["i-2 word", &context[i - 2]], &mut res);
        Self::add_feature(&["i+1 word", &context[i + 1]], &mut res);
        Self::add_feature(
            &[
                "i+1 suffix",
                &context[i - 1][context[i - 1].len() - iplus..],
            ],
            &mut res,
        );
        Self::add_feature(&["i+2 word", &context[i + 2]], &mut res);

        res
    }

    fn add_feature(args: &[&str], features: &mut HashMap<String, f64>) {
        let key = args.iter().join(" ");
        *features.entry(key).or_insert(0.0) += 1.0;
    }

    fn normalize_str(t: &str) -> String {
        if t.find('-').is_some() && t.chars().nth(0) != Some('-') {
            "!HYPHEN".to_owned()
        } else if t.parse::<usize>().is_ok() {
            if t.chars().count() == 4 {
                "!YEAR".to_owned()
            } else {
                "!DIGIT".to_owned()
            }
        } else {
            t.to_lowercase()
        }
    }
}

impl Tagger for PerceptronTagger {
    type Tag = String;

    fn tag<'a, I: IntoIterator<Item = Token<'a>>>(
        &mut self,
        tokens: I,
    ) -> Result<Vec<(Token<'a>, Self::Tag)>, SmolError> {
        self.pos(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn perceptron_empty() {
        let ts = vec![
            Token {
                term: Cow::Borrowed("test"),
                offset: 0,
                index: 0,
            },
        ];
        let mut pt = PerceptronTagger::new();

        assert_eq!(SmolErrorKind::EmptyModel, pt.tag(&ts).err().unwrap().kind());
    }
}
