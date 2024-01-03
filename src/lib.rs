extern crate serde;
// Khai báo các thư viện cần thiết
use serde::Deserialize;
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::Path;
use std::{
    future::{Future, Pending, PollFn, Ready},
    pin::Pin,
    sync::{mpsc, Arc, Mutex, RwLock},
    task::{Context, Poll, Waker},
    thread, time,
};

use rand::Rng; // Thư viện sinh số ngẫu nhiên

// Định nghĩa một lỗi chung cho module tokenizer
#[derive(Debug)]
pub enum TokenizerError {
    IoError(std::io::Error),
    ParseError(std::num::ParseIntError),
    InvalidToken(String),
    InvalidPair(String),
    InvalidConfig(String),
}

// Thực hiện chuyển đổi từ các loại lỗi khác sang TokenizerError
impl From<std::io::Error> for TokenizerError {
    fn from(error: std::io::Error) -> Self {
        TokenizerError::IoError(error)
    }
}

// Thực hiện chuyển đổi từ các loại lỗi khác sang TokenizerError
impl From<std::num::ParseIntError> for TokenizerError {
    fn from(error: std::num::ParseIntError) -> Self {
        TokenizerError::ParseError(error)
    }
}

// Thực hiện chuyển đổi từ các loại lỗi khác sang TokenizerError
impl From<serde_json::Error> for TokenizerError {
    fn from(error: serde_json::Error) -> Self {
        TokenizerError::InvalidConfig(error.to_string())
    }
}

// Định nghĩa một kết quả chung cho module tokenizer
pub type TokenizerResult<T> = Result<T, TokenizerError>;

// Định nghĩa một đơn vị ngữ liệu cơ bản nhất trong văn bản
#[derive(Clone, Debug)]
pub struct Token {
    text: String, // Chuỗi ký tự của token
}

impl Token {
    // Tạo token mới từ chuỗi ký tự
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
        }
    }

    // Trả về chuỗi ký tự của token
    pub fn text(&self) -> &str {
        &self.text
    }
}

// Thực hiện so sánh hai token bằng cách so sánh chuỗi ký tự của chúng
impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Eq for Token {}

// Thực hiện băm một token bằng cách băm chuỗi ký tự của nó
impl Hash for Token {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.text.hash(state);
    }
}

// Thực hiện hiển thị một token bằng cách hiển thị chuỗi ký tự của nó
impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

// Định nghĩa một tập hợp tất cả các token duy nhất
#[derive(Clone, Debug)]

pub struct Vocabulary {
    tokens: HashSet<Token>,       // Tập hợp các token
    index: HashMap<Token, usize>, // Ánh xạ từ token sang chỉ số
    token: Vec<Token>,            // Ánh xạ từ chỉ số sang token
    embeddings: Vec<Vec<f64>>,    // Các vector nhúng
}

impl Vocabulary {
    // Tạo một đối tượng Vocabulary mới
    pub fn new() -> Self {
        Self {
            // tokens: Vec::new(),
            embeddings: Vec::new(),
            tokens: HashSet::new(),
            index: HashMap::new(),
            token: Vec::new(),
        }
    }

    // Lấy vector nhúng tương ứng với token đã cho
    pub fn get_embedding_by_token(&self, token: &Token) -> Vec<f64> {
        let index = self.get_index(token).unwrap();
        self.get_embedding(index)
    }

    // Thêm một token mới vào từ vựng
    pub fn add_token(&mut self, token: &Token) {
        // Nếu token chưa có trong từ vựng
        if !self.tokens.contains(token) {
            // Thêm token vào từ vựng
            self.tokens.insert(token.clone());

            // Tạo một vector để lưu trữ các vector nhúng
            let mut embeddings: Vec<f64> = Vec::new();

            // Tạo các vector nhúng với phân phối chuẩn
            let mut rng = rand::thread_rng();
            for _ in 0..self.embeddings[0].len() {
                let mut embedding: Vec<f64> = Vec::new();
                for _ in 0..self.embeddings[0].len() {
                    let e: f64 = rng.gen();
                    embedding.push(e);
                }
                embeddings.push(embedding);
            }

            // Thêm vector nhúng vào vector
            self.embeddings.push(embeddings);
        }
    }

    // Lấy kích thước từ vựng
    pub fn get_size(&self) -> usize {
        self.tokens.len()
    }

    // Thêm một token mới vào từ vựng và gán cho nó một chỉ số
    pub fn add(&mut self, token: Token) {
        if !self.tokens.contains(&token) {
            let index = self.tokens.len(); // Lấy số lượng các token hiện có
            self.tokens.insert(token.clone());
            self.index.insert(token.clone(), index);
            self.token.push(token);
        }
    }

    // Loại bỏ một token từ từ vựng
    pub fn remove(&mut self, token: &Token) {
        if self.tokens.contains(token) {
            let idx = self.index.remove(token).unwrap();
            self.tokens.remove(token);
            self.token.remove(idx);
            for i in idx..self.tokens.len() {
                let t = self.token[i].clone();
                self.index.insert(t, i);
            }
        }
    }

    // Kiểm tra xem một token có tồn tại trong từ vựng hay không
    pub fn contains(&self, token: &Token) -> bool {
        self.tokens.contains(token)
    }

    // Trả về chỉ số của một token trong từ vựng
    pub fn get_index(&self, token: &Token) -> Option<usize> {
        self.index.get(token).copied()
    }

    // Trả về token tương ứng với một chỉ số trong từ vựng
    pub fn get_token(&self, index: usize) -> Option<&Token> {
        self.token.get(index)
    }

    fn get_embedding(&self, id: usize) -> Vec<f64> {
        let mut rng = rand::thread_rng();
        let mut embedding: Vec<f64> = Vec::new();
        for _ in 0..id {
            embedding.push(rng.gen_range(-1.0..1.0));
        }
        embedding
    }
}

// Định nghĩa một lớp để điều chỉnh và cải tiến từ vựng
#[derive(Clone, Debug)]
pub struct Optimizer {
    method: String,   // Phương pháp tối ưu hóa
    threshold: usize, // Ngưỡng để loại bỏ các token ít phổ biến
}

impl Optimizer {
    // Tối ưu hóa từ vựng hiện có
    pub fn optimize(&self, vocabulary: &mut Vocabulary) {
        match self.method.as_str() {
            "frequency" => self.optimize_by_frequency(vocabulary),
            "coverage" => self.optimize_by_coverage(vocabulary),
            _ => panic!("Invalid optimization method: {}", self.method),
        }
    }

    // Tối ưu hóa từ vựng bằng cách loại bỏ các token có tần suất thấp hơn ngưỡng
    fn optimize_by_frequency(&self, vocabulary: &mut Vocabulary) {
        let mut freq: HashMap<Token, usize> = HashMap::new();
        for token in vocabulary.tokens.iter() {
            let count = freq.entry(token.clone()).or_insert(0);
            *count += 1;
        }
        for (token, count) in freq.iter() {
            if *count < self.threshold {
                vocabulary.remove(token);
            }
        }
    }

    // Tối ưu hóa từ vựng bằng cách loại bỏ các token có độ phủ thấp hơn ngưỡng
    fn optimize_by_coverage(&self, vocabulary: &mut Vocabulary) {
        let mut cov: HashMap<Token, f64> = HashMap::new();
        let total: f64 = vocabulary.tokens.len() as f64;
        for token in vocabulary.tokens.iter() {
            let ratio = vocabulary.get_index(token).unwrap() as f64 / total;
            cov.insert(token.clone(), ratio);
        }
        for (token, ratio) in cov.iter() {
            if *ratio < self.threshold as f64 / 100.0 {
                vocabulary.remove(token);
            }
        }
    }
}

// Định nghĩa một lớp để quản lý cách lưu trữ và truy xuất token và cặp token từ từ vựng
#[derive(Clone, Debug)]
pub struct Storage {
    path: String,   // Đường dẫn lưu trữ từ vựng
    format: String, // Định dạng lưu trữ từ vựng
}

impl Storage {
    // Lưu từ vựng vào đường dẫn chỉ định
    pub fn save(&self, vocabulary: &Vocabulary) -> TokenizerResult<()> {
        match self.format.as_str() {
            // "json" => self.save_as_json(vocabulary),
            "csv" => self.save_as_csv(vocabulary),
            _ => panic!("Invalid storage format: {}", self.format),
        }
    }

    // Tải từ vựng từ đường dẫn đã lưu
    pub fn load(&self) -> TokenizerResult<Vocabulary> {
        match self.format.as_str() {
            // "json" => self.load_from_json(),
            "csv" => self.load_from_csv(),
            _ => panic!("Invalid storage format: {}", self.format),
        }
    }

    // // Lưu từ vựng dưới dạng json
    // fn save_as_json(&self, vocabulary: &Vocabulary) -> TokenizerResult<()> {
    //     let mut file = std::fs::File::create(&self.path)?;
    //     let json = serde_json::to_string(s)?;
    //     file.write_all(json.as_bytes())?;
    //     Ok(())
    // }

    // // Tải từ vựng từ json
    // fn load_from_json(&self) -> TokenizerResult<Vocabulary> {
    //     let mut file = std::fs::File::open(&self.path)?;
    //     let mut json = String::new();
    //     file.read_to_string(&mut json)?;
    //     let vocabulary = serde_json::from_str(&json).map_err(|err| TokenizerError::from(err))?;
    //     Ok(vocabulary)
    // }

    // Lưu từ vựng dưới dạng csv
    fn save_as_csv(&self, vocabulary: &Vocabulary) -> TokenizerResult<()> {
        let mut file = std::fs::File::create(&self.path)?;
        let mut csv = String::new();
        for token in vocabulary.tokens.iter() {
            csv.push_str(&format!(
                "{},{}\n",
                token,
                vocabulary.get_index(token).unwrap()
            ));
        }
        file.write_all(csv.as_bytes())?;
        Ok(())
    }

    // Tải từ vựng từ csv
    fn load_from_csv(&self) -> TokenizerResult<Vocabulary> {
        let mut file = std::fs::File::open(&self.path)?;
        let mut csv = String::new();
        file.read_to_string(&mut csv)?;
        let mut vocabulary = Vocabulary::new();
        for line in csv.lines() {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() == 2 {
                let token = Token::new(parts[0]);
                // let index = parts[1].parse()?;
                vocabulary.add(token);
            } else {
                return Err(TokenizerError::InvalidToken(line.to_owned()));
            }
        }
        Ok(vocabulary)
    }
}

// Định nghĩa một lớp để đánh giá hiệu suất và độ chính xác của tokenizer và BPE
#[derive(Clone, Debug)]
pub struct Evaluator {
    metric: String,    // Phép đo đánh giá
    data: Vec<String>, // Dữ liệu đánh giá
}

impl Evaluator {
    // Tạo một đánh giá mới với phép đo và dữ liệu đánh giá chỉ định
    pub fn new(metric: String, data: Vec<String>) -> Self {
        Self { metric, data }
    }

    // Đánh giá tokenizer trên tập dữ liệu và trả về kết quả
    pub fn evaluate(&self, tokenizer: &Tokenizer) -> Evaluation {
        match self.metric.as_str() {
            "perplexity" => self.evaluate_by_perplexity(tokenizer),
            "accuracy" => self.evaluate_by_accuracy(tokenizer),
            _ => panic!("Invalid evaluation metric: {}", self.metric),
        }
    }

    // Đánh giá tokenizer bằng cách tính perplexity trên tập dữ liệu
    fn evaluate_by_perplexity(&self, tokenizer: &Tokenizer) -> Evaluation {
        let mut total: f64 = 0.0;
        let mut count: usize = 0;
        for text in self.data.iter() {
            let tokens = tokenizer.tokenize(text);
            let prob = tokenizer.model.probability(&tokens);
            let ppl = prob.powf(-1.0 / tokens.len() as f64);
            total += ppl;
            count += 1;
        }
        let avg_ppl = total / count as f64;
        Evaluation {
            metric: self.metric.clone(),
            score: avg_ppl,
        }
    }

    // Đánh giá tokenizer bằng cách tính độ chính xác trên tập dữ liệu
    fn evaluate_by_accuracy(&self, tokenizer: &Tokenizer) -> Evaluation {
        let mut correct: usize = 0;
        let mut total: usize = 0;
        for text in self.data.iter() {
            let tokens = tokenizer.tokenize(text);
            let detext = tokenizer.detokenize(&tokens);
            if &detext == text {
                correct += 1;
            }
            total += 1;
        }
        let acc = correct as f64 / total as f64;
        Evaluation {
            metric: self.metric.clone(),
            score: acc,
        }
    }
}

// Định nghĩa một cấu trúc để lưu trữ kết quả đánh giá
#[derive(Clone, Debug)]
pub struct Evaluation {
    metric: String, // Phép đo đánh giá
    score: f64,     // Điểm số đánh giá
}

// Định nghĩa một lớp để điều chỉnh các tham số của BPE và tokenizer
#[derive(Clone, Debug)]
pub struct Adjuster {
    criteria: Criteria, // Tiêu chí điều chỉnh
}

impl Adjuster {
    // Tạo một điều chỉnh mới với tiêu chí chỉ định
    pub fn new(criteria: Criteria) -> Self {
        Self { criteria }
    }

    // Điều chỉnh tokenizer dựa trên các tiêu chí chỉ định
    pub fn adjust(&self, tokenizer: &mut Tokenizer) {
        match self.criteria {
            Criteria::Perplexity(target) => self.adjust_by_perplexity(tokenizer, target),
            Criteria::Accuracy(target) => self.adjust_by_accuracy(tokenizer, target),
            Criteria::VocabularySize(target) => self.adjust_by_vocabulary_size(tokenizer, target),
        }
    }

    // Điều chỉnh tokenizer bằng cách thay đổi số lượng cặp token được gộp để đạt perplexity mong muốn
    fn adjust_by_perplexity(&self, tokenizer: &mut Tokenizer, target: f64) {
        let mut low = 0;
        let mut high = tokenizer.bpe.pairs.len();
        let mut best_ppl = f64::MAX;
        let mut best_pairs = 0;
        while low <= high {
            let mid = (low + high) / 2;
            tokenizer.bpe.set_pairs(mid);
            let eval = tokenizer.evaluate();
            if eval.metric == "perplexity" {
                let ppl = eval.score;
                if ppl < best_ppl {
                    best_ppl = ppl;
                    best_pairs = mid;
                }
                if ppl < target {
                    high = mid - 1;
                } else if ppl > target {
                    low = mid + 1;
                } else {
                    break;
                }
            } else {
                panic!("Invalid evaluation metric: {}", eval.metric);
            }
        }
        tokenizer.bpe.set_pairs(best_pairs);
    }

    // Điều chỉnh tokenizer bằng cách thay đổi số lượng cặp token được gộp để đạt độ chính xác mong muốn
    fn adjust_by_accuracy(&self, tokenizer: &mut Tokenizer, target: f64) {
        let mut low = 0;
        let mut high = tokenizer.bpe.pairs.len();
        let mut best_acc = 0.0;
        let mut best_pairs = 0;
        while low <= high {
            let mid = (low + high) / 2;
            tokenizer.bpe.set_pairs(mid);
            let eval = tokenizer.evaluate();
            if eval.metric == "accuracy" {
                let acc = eval.score;
                if acc > best_acc {
                    best_acc = acc;
                    best_pairs = mid;
                }
                if acc < target {
                    low = mid + 1;
                } else if acc > target {
                    high = mid - 1;
                } else {
                    break;
                }
            } else {
                panic!("Invalid evaluation metric: {}", eval.metric);
            }
        }
        tokenizer.bpe.set_pairs(best_pairs);
    }

    // Điều chỉnh tokenizer bằng cách thay đổi số lượng cặp token được gộp để đạt kích thước từ vựng mong muốn
    fn adjust_by_vocabulary_size(&self, tokenizer: &mut Tokenizer, target: usize) {
        let mut low = 0;
        let mut high = tokenizer.bpe.pairs.len();
        let mut best_size = usize::MAX;
        let mut best_pairs = 0;
        while low <= high {
            let mid = (low + high) / 2;
            tokenizer.bpe.set_pairs(mid);
            let size = tokenizer.vocabulary.tokens.len();
            if size < best_size {
                best_size = size;
                best_pairs = mid;
            }
            if size < target {
                high = mid - 1;
            } else if size > target {
                low = mid + 1;
            } else {
                break;
            }
        }
        tokenizer.bpe.set_pairs(best_pairs);
    }
}

// Định nghĩa một liệt kê để lưu trữ các tiêu chí điều chỉnh
#[derive(Clone, Debug)]
pub enum Criteria {
    Perplexity(f64),       // Perplexity mong muốn
    Accuracy(f64),         // Độ chính xác mong muốn
    VocabularySize(usize), // Kích thước từ vựng mong muốn
}

// Định nghĩa một lớp để thực hiện thuật toán BPE
#[derive(Clone, Debug)]
pub struct BPE {
    vocabulary: Vocabulary, // Từ vựng
    pairs: Vec<Pair>,       // Danh sách các cặp token được gộp
    limit: usize,           // Giới hạn số lượng cặp token được gộp
}

impl BPE {
    // Tạo một đối tượng BPE mới
    pub fn new() -> Self {
        Self {
            vocabulary: Vocabulary::new(),
            pairs: Vec::new(),
            limit: usize::MAX,
        }
    }

    // Huấn luyện BPE trên dữ liệu đầu vào
    pub fn train(&mut self, data: &[String]) {
        // Bước 1: Tạo từ vựng ban đầu
        self.initialize_vocabulary(data);

        // Bước 2: Đếm số lần xuất hiện của mỗi cặp token liền kề
        let mut counts = self.count_pairs(data);

        // Bước 3: Chọn cặp token có số lần xuất hiện cao nhất và gộp chúng lại
        while let Some(pair) = self.find_best_pair(&counts) {
            self.merge_pair(pair.clone());
            counts = self.update_counts(&counts, &pair);
            if self.pairs.len() >= self.limit {
                break;
            }
        }
    }
    // Tìm cặp token có số lần xuất hiện cao nhất trong bảng băm
    fn find_best_pair(&self, counts: &HashMap<Pair, usize>) -> Option<Pair> {
        // Tạo một biến để lưu trữ cặp token tốt nhất và số lần xuất hiện của nó
        let mut best_pair: Option<Pair> = None;
        let mut best_count: usize = 0;

        // Duyệt qua mỗi cặp token và số lần xuất hiện của nó trong bảng băm
        for (pair, count) in counts.iter() {
            // Kiểm tra xem số lần xuất hiện có lớn hơn số lần xuất hiện tốt nhất hay không
            if *count > best_count {
                // Nếu có, cập nhật cặp token tốt nhất và số lần xuất hiện của nó
                best_pair = Some(pair.clone());
                best_count = *count;
            }
        }

        // Trả về cặp token tốt nhất
        best_pair
    }

    // Gộp một cặp token vào từ vựng và thêm nó vào danh sách các cặp token được gộp
    fn merge_pair(&mut self, pair: Pair) {
        // Tạo một token mới từ cặp token
        let new_token = Token::new(&format!("{}{}", pair.first(), pair.second()));

        // Thêm token mới vào từ vựng
        self.vocabulary.add(new_token);

        // Thêm cặp token vào danh sách các cặp token được gộp
        self.pairs.push(pair);
    }

    // Cập nhật bảng băm số lần xuất hiện của các cặp token sau khi gộp một cặp token
    fn update_counts(&self, counts: &HashMap<Pair, usize>, pair: &Pair) -> HashMap<Pair, usize> {
        // Tạo một bảng băm mới để lưu trữ số lần xuất hiện của các cặp token sau khi gộp
        let mut new_counts: HashMap<Pair, usize> = HashMap::new();

        // Duyệt qua mỗi cặp token và số lần xuất hiện của nó trong bảng băm cũ
        for (old_pair, old_count) in counts.iter() {
            // Kiểm tra xem cặp token cũ có chứa cặp token đã gộp hay không
            if old_pair.first() == pair.first() && old_pair.second() == pair.second() {
                // Nếu có, bỏ qua cặp token cũ
                continue;
            } else if old_pair.first() == pair.first() {
                // Nếu cặp token cũ có token đầu tiên trùng với token đầu tiên của cặp token đã gộp
                // Tạo một cặp token mới từ token mới và token thứ hai của cặp token cũ
                let new_pair = Pair::new(
                    Token::new(&format!("{}{}", pair.first(), pair.second())),
                    old_pair.second().clone(),
                );

                // Tăng số lần xuất hiện của cặp token mới lên bằng số lần xuất hiện của cặp token cũ
                let new_count = new_counts.entry(new_pair).or_insert(0);
                *new_count += old_count;
            } else if old_pair.second() == pair.second() {
                // Nếu cặp token cũ có token thứ hai trùng với token thứ hai của cặp token đã gộp
                // Tạo một cặp token mới từ token đầu tiên của cặp token cũ và token mới
                let new_pair = Pair::new(
                    old_pair.first().clone(),
                    Token::new(&format!("{}{}", pair.first(), pair.second())),
                );

                // Tăng số lần xuất hiện của cặp token mới lên bằng số lần xuất hiện của cặp token cũ
                let new_count = new_counts.entry(new_pair).or_insert(0);
                *new_count += old_count;
            } else {
                // Nếu cặp token cũ không chứa cặp token đã gộp
                // Giữ nguyên cặp token cũ và số lần xuất hiện của nó
                new_counts.insert(old_pair.clone(), *old_count);
            }
        }

        // Trả về bảng băm mới
        new_counts
    }

    // Mã hóa một chuỗi văn bản thành chuỗi các token
    pub fn encode(&self, text: &str) -> Vec<Token> {
        // Tách văn bản thành các từ và thêm ký tự đặc biệt "</w>" vào cuối mỗi từ
        let words: Vec<String> = text
            .split_whitespace()
            .map(|w| format!("{}{}", w, "</w>"))
            .collect();

        // Tách mỗi từ thành các token ban đầu
        let mut tokens: Vec<Vec<Token>> = words.iter().map(|w| self.split_word(w)).collect();

        // Gộp các token theo thứ tự ưu tiên của các cặp token
        for pair in self.pairs.iter() {
            for word in tokens.iter_mut() {
                self.replace_pair(word, pair);
            }
        }

        // Nối các token của các từ lại thành một chuỗi token
        tokens.concat()
    }

    // Giải mã chuỗi các token thành văn bản
    pub fn decode(&self, tokens: &[Token]) -> String {
        // Nối các token lại thành một chuỗi ký tự
        let mut text = tokens.iter().map(|t| t.text()).collect::<String>();

        // Loại bỏ ký tự đặc biệt "</w>" khỏi cuối mỗi từ
        text = text.replace("</w>", " ");

        // Loại bỏ khoảng trắng thừa
        text = text.trim().to_owned();

        // Trả về văn bản
        text
    }

    // Tạo từ vựng ban đầu bằng cách liệt kê tất cả các ký tự duy nhất trong văn bản
    fn initialize_vocabulary(&mut self, data: &[String]) {
        // Tạo một tập hợp để lưu trữ các ký tự duy nhất
        let mut chars: HashSet<char> = HashSet::new();

        // Duyệt qua mỗi chuỗi văn bản trong dữ liệu
        for text in data.iter() {
            // Duyệt qua mỗi ký tự trong văn bản
            for c in text.chars() {
                // Thêm ký tự vào tập hợp nếu chưa có
                chars.insert(c);
            }
        }

        // Thêm ký tự đặc biệt "</w>" vào tập hợp
        chars.insert('<');
        chars.insert('/');
        chars.insert('w');
        chars.insert('>');

        // Tạo một từ vựng mới từ tập hợp các ký tự
        self.vocabulary = Vocabulary::new();
        for c in chars.iter() {
            let token = Token::new(&c.to_string());
            self.vocabulary.add(token);
        }
    }

    // Đếm số lần xuất hiện của mỗi cặp token liền kề trong văn bản
    fn count_pairs(&self, data: &[String]) -> HashMap<Pair, usize> {
        // Tạo một bảng băm để lưu trữ số lần xuất hiện của mỗi cặp token
        let mut counts: HashMap<Pair, usize> = HashMap::new();

        // Duyệt qua mỗi chuỗi văn bản trong dữ liệu
        for text in data.iter() {
            // Tách mỗi từ thành các token ban đầu
            let words: Vec<Vec<Token>> = text
                .split_whitespace()
                .map(|w| self.split_word(w))
                .collect();

            // Duyệt qua mỗi từ trong văn bản
            for word in words.iter() {
                // Duyệt qua mỗi cặp token liền kề trong từ
                // for pair in word.windows(2) {
                //     // Tăng số lần xuất hiện của cặp token lên 1
                //     let count = counts.entry(pair.to_vec()).or_insert(0);
                //     *count += 1;
                // }
                // Duyệt qua mỗi cặp token liền kề trong từ
                for pair in word.windows(2) {
                    // Tạo một cặp token từ vector token
                    let pair = Pair::new(pair[0].clone(), pair[1].clone());

                    // Tăng số lần xuất hiện của cặp token lên 1
                    let count = counts.entry(pair).or_insert(0);
                    *count += 1;
                }
            }
        }

        // Trả về bảng băm
        counts
    }

    // Tách một từ thành các token ban đầu
    fn split_word(&self, word: &str) -> Vec<Token> {
        // Tạo một vector để lưu trữ các token
        let mut tokens: Vec<Token> = Vec::new();

        // Duyệt qua mỗi ký tự trong từ
        for c in word.chars() {
            // Tạo một token từ ký tự
            let token = Token::new(&c.to_string());

            // Kiểm tra xem token có tồn tại trong từ vựng hay không
            if self.vocabulary.contains(&token) {
                // Thêm token vào vector
                tokens.push(token);
            } else {
                // Nếu không, trả về lỗi
                panic!("Invalid token: {}", token);
            }
        }

        // Trả về vector các token
        tokens
    }

    // Thay thế một cặp token bằng một token mới trong một vector token
    fn replace_pair(&self, tokens: &mut Vec<Token>, pair: &Pair) {
        // Tạo một vector để lưu trữ các token sau khi thay thế
        let mut new_tokens: Vec<Token> = Vec::new();

        // Tạo một biến để lưu trữ token trước đó
        let mut prev: Option<Token> = None;

        // Duyệt qua mỗi token trong vector
        for token in tokens.iter() {
            // Nếu có token trước đó
            if let Some(p) = prev {
                // Tạo một cặp token từ token trước đó và token hiện tại
                let current_pair = Pair::new(p.clone(), token.clone());

                // Kiểm tra xem cặp token có trùng với cặp token cần thay thế hay không
                if current_pair == *pair {
                    // Nếu có, tạo một token mới từ cặp token
                    let new_token = Token::new(&format!("{}{}", p, token));

                    // Thêm token mới vào vector
                    new_tokens.push(new_token);

                    // Đặt token trước đó là None
                    prev = None;
                } else {
                    // Nếu không, thêm token trước đó vào vector
                    new_tokens.push(p);

                    // Đặt token trước đó là token hiện tại
                    prev = Some(token.clone());
                }
            } else {
                // Nếu không có token trước đó, đặt token trước đó là token hiện tại
                prev = Some(token.clone());
            }
        }

        // Nếu còn token trước đó, thêm nó vào vector
        if let Some(p) = prev {
            new_tokens.push(p);
        }

        // Cập nhật vector token bằng vector mới
        *tokens = new_tokens;
    }

    // Đặt số lượng cặp token được gộp
    fn set_pairs(&mut self, n: usize) {
        // Kiểm tra xem n có nhỏ hơn hoặc bằng số lượng cặp token đã gộp hay không
        if n <= self.pairs.len() {
            // Nếu có, cắt bớt vector cặp token để chỉ giữ lại n phần tử đầu tiên
            self.pairs.truncate(n);
        } else {
            // Nếu không, trả về lỗi
            panic!("Invalid number of pairs: {}", n);
        }
    }
}

// Định nghĩa một lớp để điều khiển và quản lý quá trình mã hóa và giải mã văn bản
#[derive(Clone, Debug)]
pub struct Tokenizer {
    bpe: BPE,               // Thuật toán BPE
    vocabulary: Vocabulary, // Từ vựng
    model: GPT,             // Mô hình ngôn ngữ GPT
    config: Config,         // Cấu hình
}

impl Tokenizer {
    // Tạo tokenizer mới với thuật toán BPE đã cho
    pub fn new(bpe: BPE) -> Self {
        let vocabulary = bpe.vocabulary.clone();
        let model = GPT::new(vocabulary.tokens.len());
        let config = Config::default();
        Self {
            bpe,
            vocabulary,
            model,
            config,
        }
    }

    // Chuyển đổi văn bản thành chuỗi token
    pub fn tokenize(&self, text: &str) -> Vec<Token> {
        self.bpe.encode(text)
    }

    // Chuyển chuỗi token trở lại thành văn bản
    pub fn detokenize(&self, tokens: &[Token]) -> String {
        self.bpe.decode(tokens)
    }

    // Chuyển đổi chuỗi token thành chuỗi số nguyên hoặc vector
    pub fn convert(&self, tokens: &[Token]) -> Vec<usize> {
        self.model.convert(tokens)
    }

    // Chuyển đổi chuỗi số nguyên hoặc vector thành chuỗi token
    pub fn revert(&self, ids: &[usize]) -> Vec<Token> {
        self.model.revert(ids)
    }

    // Đánh giá tokenizer trên tập dữ liệu và trả về kết quả
    pub fn evaluate(&self) -> Evaluation {
        let evaluator = Evaluator::new(self.config.metric.clone(), self.config.data.clone());
        evaluator.evaluate(self)
    }

    // Điều chỉnh tokenizer dựa trên các tiêu chí chỉ định
    pub fn adjust(&mut self, criteria: Criteria) {
        let adjuster = Adjuster::new(criteria);
        adjuster.adjust(self);
    }
}

// Định nghĩa một cấu trúc để lưu trữ cấu hình cho tokenizer
#[derive(Clone, Debug)]
pub struct Config {
    metric: String,    // Phép đo đánh giá
    data: Vec<String>, // Dữ liệu đánh giá
    path: String,      // Đường dẫn lưu trữ từ vựng
    format: String,    // Định dạng lưu trữ từ vựng
    method: String,    // Phương pháp tối ưu hóa
    threshold: usize,  // Ngưỡng để loại bỏ các token ít phổ biến
}

impl Config {
    // Tạo cấu hình mới từ các tùy chọn
    pub fn new(
        metric: String,
        data: Vec<String>,
        path: String,
        format: String,
        method: String,
        threshold: usize,
    ) -> Self {
        Self {
            metric,
            data,
            path,
            format,
            method,
            threshold,
        }
    }

    // Tạo cấu hình mặc định
    pub fn default() -> Self {
        Self {
            metric: "perplexity".to_owned(),
            data: vec![],
            path: "vocabulary.json".to_owned(),
            format: "json".to_owned(),
            method: "frequency".to_owned(),
            threshold: 10,
        }
    }

    // Cập nhật một tùy chọn trong cấu hình
    pub fn update(&mut self, cfg: Cfg) {
        match cfg {
            Cfg::Metric(metric) => self.metric = metric,
            Cfg::Data(data) => self.data = data,
            Cfg::Path(path) => self.path = path,
            Cfg::Format(format) => self.format = format,
            Cfg::Method(method) => self.method = method,
            Cfg::Threshold(threshold) => self.threshold = threshold,
        }
    }
}

// Định nghĩa một liệt kê để lưu trữ các tùy chọn cấu hình
#[derive(Clone, Debug)]
pub enum Cfg {
    Metric(String),    // Phép đo đánh giá
    Data(Vec<String>), // Dữ liệu đánh giá
    Path(String),      // Đường dẫn lưu trữ từ vựng
    Format(String),    // Định dạng lưu trữ từ vựng
    Method(String),    // Phương pháp tối ưu hóa
    Threshold(usize),  // Ngưỡng để loại bỏ các token ít phổ biến
}
// Định nghĩa một cấu trúc để lưu trữ một cặp token
#[derive(Clone, Debug)]
pub struct Pair {
    first: Token,  // Token đầu tiên trong cặp
    second: Token, // Token thứ hai trong cặp
}

impl Pair {
    // Tạo một cặp token mới từ hai token đã cho
    pub fn new(first: Token, second: Token) -> Self {
        Self { first, second }
    }

    // Trả về token đầu tiên trong cặp
    pub fn first(&self) -> &Token {
        &self.first
    }

    // Trả về token thứ hai trong cặp
    pub fn second(&self) -> &Token {
        &self.second
    }
}

// Thực hiện so sánh hai cặp token bằng cách so sánh hai token trong cặp
impl PartialEq for Pair {
    fn eq(&self, other: &Self) -> bool {
        self.first == other.first && self.second == other.second
    }
}

impl Eq for Pair {}

// Thực hiện băm một cặp token bằng cách băm hai token trong cặp
impl Hash for Pair {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.first.hash(state);
        self.second.hash(state);
    }
}

// Thực hiện hiển thị một cặp token bằng cách hiển thị hai token trong cặp
impl fmt::Display for Pair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.first, self.second)
    }
}

// Định nghĩa một lớp để thực hiện mô hình ngôn ngữ GPT
#[derive(Clone, Debug)]
pub struct GPT {
    model: Transformer, // Mô hình Transformer
    vocab_size: usize,  // Kích thước từ vựng
}

impl GPT {
    // Tạo một đối tượng GPT mới với kích thước từ vựng đã cho
    pub fn new(vocab_size: usize) -> Self {
        // Tạo một mô hình Transformer với các tham số mặc định
        let model = Transformer::new(
            vocab_size, // Kích thước từ vựng
            12,         // Số lớp mã hóa
            12,         // Số đầu trong mỗi lớp
            768,        // Kích thước trạng thái ẩn
            0.1,        // Tỉ lệ dropout
                        // 12,         // Số lớp giải mã
        );

        Self { model, vocab_size }
    }

    // Chuyển đổi chuỗi token thành chuỗi số nguyên hoặc vector
    pub fn convert(&self, tokens: &[Token]) -> Vec<usize> {
        // Tạo một vector để lưu trữ các số nguyên
        let mut ids: Vec<usize> = Vec::new();

        // Duyệt qua mỗi token trong chuỗi
        for token in tokens.iter() {
            // Lấy chỉ số của token trong từ vựng
            let id = self.model.vocabulary.get_index(token).unwrap();

            // Thêm số nguyên vào vector
            ids.push(id);
        }

        // Trả về vector các số nguyên
        ids
    }

    // Chuyển đổi chuỗi số nguyên hoặc vector thành chuỗi token
    pub fn revert(&self, ids: &[usize]) -> Vec<Token> {
        // Tạo một vector để lưu trữ các token
        let mut tokens: Vec<Token> = Vec::new();

        // Duyệt qua mỗi số nguyên trong chuỗi
        for id in ids.iter() {
            // Lấy token tương ứng với số nguyên trong từ vựng
            let token = self.model.vocabulary.get_token(*id).unwrap();

            // Thêm token vào vector
            tokens.push(token.clone());
        }

        // Trả về vector các token
        tokens
    }

    // Tính xác suất của một chuỗi token dựa trên mô hình ngôn ngữ
    pub fn probability(&self, tokens: &[Token]) -> f64 {
        // Chuyển đổi chuỗi token thành chuỗi số nguyên
        let ids = self.convert(tokens);

        // Tính xác suất của chuỗi số nguyên dựa trên mô hình Transformer
        self.model.probability(&ids)
    }

    // Sinh văn bản tiếp theo dựa trên một chuỗi token đã cho
    pub fn generate(&self, tokens: &[Token], length: usize) -> Vec<Token> {
        // Chuyển đổi chuỗi token thành chuỗi số nguyên
        let mut ids = self.convert(tokens);

        // Sinh văn bản tiếp theo dựa trên mô hình Transformer
        self.model.generate(&mut ids, length);

        // Chuyển đổi chuỗi số nguyên thành chuỗi token
        self.revert(&ids)
    }
}
// Định nghĩa một lớp để thực hiện mô hình Transformer
#[derive(Clone, Debug)]
pub struct Transformer {
    vocabulary: Vocabulary, // Từ vựng
    encoder: Encoder,       // Mã hóa
    decoder: Decoder,       // Giải mã
}

impl Transformer {
    // Tạo một đối tượng Transformer mới với các tham số đã cho
    pub fn new(
        vocab_size: usize,  // Kích thước từ vựng
        num_layers: usize,  // Số lớp mã hóa và giải mã
        num_heads: usize,   // Số đầu trong mỗi lớp
        hidden_size: usize, // Kích thước trạng thái ẩn
        dropout: f64,       // Tỉ lệ dropout
    ) -> Self {
        // Tạo một từ vựng mới với kích thước đã cho
        let vocabulary = Vocabulary::new();

        // Tạo một mã hóa mới với các tham số đã cho
        let encoder = Encoder::new(num_layers, num_heads, hidden_size, dropout);

        // Tạo một giải mã mới với các tham số đã cho
        let decoder = Decoder::new(num_layers, num_heads, hidden_size, dropout);

        Self {
            vocabulary,
            encoder,
            decoder,
        }
    }

    // Tính xác suất của một chuỗi số nguyên dựa trên mô hình ngôn ngữ
    pub fn probability(&self, ids: &[usize]) -> f64 {
        // Tạo một vector để lưu trữ các xác suất của từng số nguyên
        let mut probs: Vec<f64> = Vec::new();

        // Tạo một biến để lưu trữ trạng thái ẩn của mã hóa
        let mut encoder_state: Option<EncoderState> = None;

        // Duyệt qua mỗi số nguyên trong chuỗi
        for i in 0..ids.len() {
            // Lấy số nguyên hiện tại
            let id = ids[i];

            // Tạo một vector để lưu trữ số nguyên hiện tại và các số nguyên trước đó
            let input_ids = ids[..i + 1].to_vec();

            // Tạo một vector để lưu trữ số nguyên hiện tại
            let target_ids = vec![id];

            // Tính toán đầu ra của mã hóa và giải mã
            let (encoder_output, decoder_output) =
                self.forward(&input_ids, &target_ids, encoder_state);

            // Cập nhật trạng thái ẩn của mã hóa
            encoder_state = encoder_output.state;

            // Lấy xác suất của số nguyên hiện tại từ đầu ra của giải mã
            let prob = decoder_output.logits[0][id];

            // Thêm xác suất vào vector
            probs.push(prob);
        }

        // Tính tích của các xác suất
        let mut product: f64 = 1.0;
        for p in probs.iter() {
            product *= p;
        }

        // Trả về tích của các xác suất
        product
    }

    // Sinh văn bản tiếp theo dựa trên một chuỗi số nguyên đã cho
    pub fn generate(&self, ids: &mut Vec<usize>, length: usize) {
        // Tạo một biến để lưu trữ trạng thái ẩn của mã hóa
        let mut encoder_state: Option<EncoderState> = None;

        // Sinh văn bản tiếp theo với độ dài đã cho
        for _ in 0..length {
            // Tính toán đầu ra của mã hóa và giải mã
            let (encoder_output, decoder_output) = self.forward(ids, ids, encoder_state);

            // Cập nhật trạng thái ẩn của mã hóa
            encoder_state = encoder_output.state;

            // Lấy số nguyên có xác suất cao nhất từ đầu ra của giải mã
            let id = decoder_output.logits[0]
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                .unwrap()
                .0;

            // Thêm số nguyên vào chuỗi
            ids.push(id);
        }
    }

    // Tính toán đầu ra của mã hóa và giải mã
    fn forward(
        &self,
        input_ids: &[usize],                 // Chuỗi số nguyên đầu vào
        target_ids: &[usize],                // Chuỗi số nguyên đích
        encoder_state: Option<EncoderState>, // Trạng thái ẩn của mã hóa
    ) -> (EncoderOutput, DecoderOutput) {
        // Tạo một vector để lưu trữ các vector nhúng của số nguyên đầu vào
        let mut input_embeddings: Vec<Vec<f64>> = Vec::new();

        // Duyệt qua mỗi số nguyên đầu vào
        for id in input_ids.iter() {
            // Lấy vector nhúng tương ứng với số nguyên trong từ vựng
            let embedding = self.vocabulary.get_embedding(*id);

            // Thêm vector nhúng vào vector
            input_embeddings.push(embedding);
        }

        // Tạo một vector để lưu trữ các vector nhúng của số nguyên đích
        let mut target_embeddings: Vec<Vec<f64>> = Vec::new();

        // Duyệt qua mỗi số nguyên đích
        for id in target_ids.iter() {
            // Lấy vector nhúng tương ứng với số nguyên trong từ vựng
            let embedding = self.vocabulary.get_embedding(*id);

            // Thêm vector nhúng vào vector
            target_embeddings.push(embedding);
        }

        // Tính toán đầu ra của mã hóa
        let encoder_output = self.encoder.forward(&input_embeddings, encoder_state);

        // Tính toán đầu ra của giải mã
        let decoder_output = self.decoder.forward(&target_embeddings, &encoder_output);

        // Trả về đầu ra của mã hóa và giải mã
        (encoder_output, decoder_output)
    }
}

// Định nghĩa một lớp để thực hiện mã hóa
#[derive(Clone, Debug)]
pub struct Encoder {
    layers: Vec<EncoderLayer>, // Các lớp mã hóa
    dropout: Dropout,          // Dropout
}

impl Encoder {
    // Tạo một đối tượng Encoder mới với các tham số đã cho
    pub fn new(
        num_layers: usize,  // Số lớp mã hóa
        num_heads: usize,   // Số đầu trong mỗi lớp
        hidden_size: usize, // Kích thước trạng thái ẩn
        dropout: f64,       // Tỉ lệ dropout
    ) -> Self {
        // Tạo một vector để lưu trữ các lớp mã hóa
        let mut layers: Vec<EncoderLayer> = Vec::new();

        // Tạo một lớp mã hóa mới với các tham số đã cho
        let layer = EncoderLayer::new(num_heads, hidden_size, dropout);

        // Thêm lớp mã hóa vào vector
        layers.push(layer);

        // Tạo các lớp mã hóa khác với các tham số đã cho
        for _ in 1..num_layers {
            let layer = EncoderLayer::new(num_heads, hidden_size, dropout);
            layers.push(layer);
        }

        // Tạo một lớp dropout với tỉ lệ dropout đã cho
        let dropout = Dropout::new(dropout);

        Self { layers, dropout }
    }

    // Tính toán đầu ra của mã hóa
    pub fn forward(
        &self,
        embeddings: &[Vec<f64>],     // Vector các vector nhúng đầu vào
        state: Option<EncoderState>, // Trạng thái ẩn của mã hóa
    ) -> EncoderOutput {
        // Tạo một vector để lưu trữ các vector nhúng đầu vào
        let mut embeddings: Vec<Vec<f64>> = embeddings.to_vec();

        // Tính toán đầu ra của các lớp mã hóa
        for layer in self.layers.iter() {
            embeddings = layer.forward(&embeddings);
        }

        // Tính toán đầu ra của lớp dropout
        let embeddings = self.dropout.forward(&embeddings);

        // Trả về đầu ra của mã hóa
        EncoderOutput { embeddings, state }
    }
}

// Định nghĩa một lớp để thực hiện giải mã
#[derive(Clone, Debug)]
pub struct Decoder {
    layers: Vec<DecoderLayer>, // Các lớp giải mã
    dropout: Dropout,          // Dropout
}

impl Decoder {
    // Tạo một đối tượng Decoder mới với các tham số đã cho
    pub fn new(
        num_layers: usize,  // Số lớp giải mã
        num_heads: usize,   // Số đầu trong mỗi lớp
        hidden_size: usize, // Kích thước trạng thái ẩn
        dropout: f64,       // Tỉ lệ dropout
    ) -> Self {
        // Tạo một vector để lưu trữ các lớp giải mã
        let mut layers: Vec<DecoderLayer> = Vec::new();

        // Tạo một lớp giải mã mới với các tham số đã cho
        let layer = DecoderLayer::new(num_heads, hidden_size, dropout);

        // Thêm lớp giải mã vào vector
        layers.push(layer);

        // Tạo các lớp giải mã khác với các tham số đã cho
        for _ in 1..num_layers {
            let layer = DecoderLayer::new(num_heads, hidden_size, dropout);
            layers.push(layer);
        }

        // Tạo một lớp dropout với tỉ lệ dropout đã cho
        let dropout = Dropout::new(dropout);

        Self { layers, dropout }
    }

    // Tính toán đầu ra của giải mã
    pub fn forward(
        &self,
        embeddings: &[Vec<f64>],        // Vector các vector nhúng đầu vào
        encoder_output: &EncoderOutput, // Đầu ra của mã hóa
    ) -> DecoderOutput {
        // Tạo một vector để lưu trữ các vector nhúng đầu vào
        let mut embeddings: Vec<Vec<f64>> = embeddings.to_vec();

        // Tính toán đầu ra của các lớp giải mã
        for layer in self.layers.iter() {
            embeddings = layer.forward(&embeddings, &encoder_output.embeddings);
        }

        // Tính toán đầu ra của lớp dropout
        let embeddings = self.dropout.forward(&embeddings);

        // Trả về đầu ra của giải mã
        DecoderOutput { logits: embeddings }
    }
}

// Định nghĩa một lớp để thực hiện lớp giải mã đầu ra
#[derive(Clone, Debug)]
pub struct DecoderOutput {
    logits: Vec<Vec<f64>>, // Đầu ra của lớp giải mã
}

// Định nghĩa một lớp để thực hiện lớp mã hóa đầu ra
#[derive(Clone, Debug)]
pub struct EncoderOutput {
    embeddings: Vec<Vec<f64>>,   // Đầu ra của lớp mã hóa
    state: Option<EncoderState>, // Trạng thái ẩn của mã hóa
}

// Định nghĩa một lớp để thực hiện trạng thái ẩn của mã hóa
#[derive(Clone, Debug)]
pub struct EncoderState {
    hidden_states: Vec<Vec<f64>>, // Trạng thái ẩn của mã hóa
}

// Định nghĩa một lớp để thực hiện lớp mã hóa
#[derive(Clone, Debug)]
pub struct EncoderLayer {
    self_attention: SelfAttention, // Tự chú ý
    feed_forward: FeedForward,     // Lan truyền tiến
    dropout: Dropout,              // Dropout
}

impl EncoderLayer {
    // Tạo một đối tượng EncoderLayer mới với các tham số đã cho
    pub fn new(num_heads: usize, hidden_size: usize, dropout: f64) -> Self {
        // Tạo một lớp tự chú ý mới với các tham số đã cho
        let self_attention = SelfAttention::new(num_heads, hidden_size, dropout);

        // Tạo một lớp lan truyền tiến mới với các tham số đã cho
        let feed_forward = FeedForward::new(hidden_size, dropout);

        // Tạo một lớp dropout với tỉ lệ dropout đã cho
        let dropout = Dropout::new(dropout);

        Self {
            self_attention,
            feed_forward,
            dropout,
        }
    }

    // Tính toán đầu ra của lớp mã hóa
    pub fn forward(&self, embeddings: &[Vec<f64>]) -> Vec<Vec<f64>> {
        // Tính toán đầu ra của lớp tự chú ý
        let embeddings = self
            .self_attention
            .forward(embeddings, embeddings, embeddings);

        // Tính toán đầu ra của lớp dropout
        let embeddings = self.dropout.forward(&embeddings);

        // Tính toán đầu ra của lớp lan truyền tiến
        let embeddings = self.feed_forward.forward(&embeddings);

        // Tính toán đầu ra của lớp dropout
        let embeddings = self.dropout.forward(&embeddings);

        // Trả về đầu ra của lớp mã hóa
        embeddings
    }
}

// Định nghĩa một lớp để thực hiện lớp giải mã
#[derive(Clone, Debug)]
pub struct DecoderLayer {
    self_attention: SelfAttention,   // Tự chú ý
    cross_attention: CrossAttention, // Chú ý chéo
    feed_forward: FeedForward,       // Lan truyền tiến
    dropout: Dropout,                // Dropout
}

impl DecoderLayer {
    // Tạo một đối tượng DecoderLayer mới với các tham số đã cho
    pub fn new(num_heads: usize, hidden_size: usize, dropout: f64) -> Self {
        // Tạo một lớp tự chú ý mới với các tham số đã cho
        let self_attention = SelfAttention::new(num_heads, hidden_size, dropout);

        // Tạo một lớp chú ý chéo mới với các tham số đã cho
        let cross_attention = CrossAttention::new(num_heads, hidden_size, dropout);

        // Tạo một lớp lan truyền tiến mới với các tham số đã cho
        let feed_forward = FeedForward::new(hidden_size, dropout);

        // Tạo một lớp dropout với tỉ lệ dropout đã cho
        let dropout = Dropout::new(dropout);

        Self {
            self_attention,
            cross_attention,
            feed_forward,
            dropout,
        }
    }

    // Tính toán đầu ra của lớp giải mã
    pub fn forward(
        &self,
        embeddings: &[Vec<f64>],         // Vector các vector nhúng đầu vào
        encoder_embeddings: &[Vec<f64>], // Vector các vector nhúng của mã hóa
    ) -> Vec<Vec<f64>> {
        // Tính toán đầu ra của lớp tự chú ý
        let embeddings = self
            .self_attention
            .forward(embeddings, embeddings, embeddings);

        // Tính toán đầu ra của lớp dropout
        let embeddings = self.dropout.forward(&embeddings);

        // Tính toán đầu ra của lớp chú ý chéo
        let embeddings =
            self.cross_attention
                .forward(&embeddings, encoder_embeddings, encoder_embeddings);

        // Tính toán đầu ra của lớp dropout
        let embeddings = self.dropout.forward(&embeddings);

        // Tính toán đầu ra của lớp lan truyền tiến
        let embeddings = self.feed_forward.forward(&embeddings);

        // Tính toán đầu ra của lớp dropout
        let embeddings = self.dropout.forward(&embeddings);

        // Trả về đầu ra của lớp giải mã
        embeddings
    }
}

// Định nghĩa một lớp để thực hiện tự chú ý
#[derive(Clone, Debug)]
pub struct SelfAttention {
    query: Linear,    // Tuyến tính
    key: Linear,      // Tuyến tính
    value: Linear,    // Tuyến tính
    dropout: Dropout, // Dropout
}

impl SelfAttention {
    // Tạo một đối tượng SelfAttention mới với các tham số đã cho
    pub fn new(num_heads: usize, hidden_size: usize, dropout: f64) -> Self {
        // Tạo một lớp tuyến tính mới với các tham số đã cho
        let query = Linear::new(hidden_size, hidden_size, false);

        // Tạo một lớp tuyến tính mới với các tham số đã cho
        let key = Linear::new(hidden_size, hidden_size, false);

        // Tạo một lớp tuyến tính mới với các tham số đã cho
        let value = Linear::new(hidden_size, hidden_size, false);

        // Tạo một lớp dropout với tỉ lệ dropout đã cho
        let dropout = Dropout::new(dropout);

        Self {
            query,
            key,
            value,
            dropout,
        }
    }

    // Tính toán đầu ra của tự chú ý
    pub fn forward(
        &self,
        embeddings: &[Vec<f64>], // Vector các vector nhúng đầu vào
        queries: &[Vec<f64>],    // Vector các vector truy vấn
        keys: &[Vec<f64>],       // Vector các vector khóa
    ) -> Vec<Vec<f64>> {
        // Tính toán đầu ra của lớp tuyến tính
        let queries = self.query.forward(queries);

        // Tính toán đầu ra của lớp tuyến tính
        let keys = self.key.forward(keys);

        // Tính toán đầu ra của lớp tuyến tính
        let values = self.value.forward(embeddings);

        // Tính toán đầu ra của lớp dropout
        let queries = self.dropout.forward(&queries);

        // Tính toán đầu ra của lớp dropout
        let keys = self.dropout.forward(&keys);

        // Tính toán đầu ra của lớp dropout
        let values = self.dropout.forward(&values);

        // Tính toán đầu ra của lớp tự chú ý
        let mut outputs: Vec<Vec<f64>> = Vec::new();
        for i in 0..queries.len() {
            let mut output: Vec<f64> = Vec::new();
            for j in 0..queries[i].len() {
                let mut sum: f64 = 0.0;
                for k in 0..queries[i].len() {
                    sum += queries[i][j] * keys[i][k] * values[i][k];
                }
                output.push(sum);
            }
            outputs.push(output);
        }

        // Trả về đầu ra của tự chú ý
        outputs
    }
}

// Định nghĩa một lớp để thực hiện chú ý chéo
#[derive(Clone, Debug)]
pub struct CrossAttention {
    query: Linear,    // Tuyến tính
    key: Linear,      // Tuyến tính
    value: Linear,    // Tuyến tính
    dropout: Dropout, // Dropout
}

impl CrossAttention {
    // Tạo một đối tượng CrossAttention mới với các tham số đã cho
    pub fn new(num_heads: usize, hidden_size: usize, dropout: f64) -> Self {
        // Tạo một lớp tuyến tính mới với các tham số đã cho
        let query = Linear::new(hidden_size, hidden_size, false);

        // Tạo một lớp tuyến tính mới với các tham số đã cho
        let key = Linear::new(hidden_size, hidden_size, false);

        // Tạo một lớp tuyến tính mới với các tham số đã cho
        let value = Linear::new(hidden_size, hidden_size, false);

        // Tạo một lớp dropout với tỉ lệ dropout đã cho
        let dropout = Dropout::new(dropout);

        Self {
            query,
            key,
            value,
            dropout,
        }
    }

    // Tính toán đầu ra của chú ý chéo
    pub fn forward(
        &self,
        embeddings: &[Vec<f64>], // Vector các vector nhúng đầu vào
        queries: &[Vec<f64>],    // Vector các vector truy vấn
        keys: &[Vec<f64>],       // Vector các vector khóa
    ) -> Vec<Vec<f64>> {
        // Tính toán đầu ra của lớp tuyến tính
        let queries = self.query.forward(queries);

        // Tính toán đầu ra của lớp tuyến tính
        let keys = self.key.forward(keys);

        // Tính toán đầu ra của lớp tuyến tính
        let values = self.value.forward(embeddings);

        // Tính toán đầu ra của lớp dropout
        let queries = self.dropout.forward(&queries);

        // Tính toán đầu ra của lớp dropout
        let keys = self.dropout.forward(&keys);

        // Tính toán đầu ra của lớp dropout
        let values = self.dropout.forward(&values);

        // Tính toán đầu ra của lớp chú ý chéo
        let mut outputs: Vec<Vec<f64>> = Vec::new();
        for i in 0..queries.len() {
            let mut output: Vec<f64> = Vec::new();
            for j in 0..queries[i].len() {
                let mut sum: f64 = 0.0;
                for k in 0..queries[i].len() {
                    sum += queries[i][j] * keys[i][k] * values[i][k];
                }
                output.push(sum);
            }
            outputs.push(output);
        }

        // Trả về đầu ra của chú ý chéo
        outputs
    }
}

// Định nghĩa một lớp để thực hiện lan truyền tiến
#[derive(Clone, Debug)]
pub struct FeedForward {
    linear1: Linear,  // Tuyến tính
    linear2: Linear,  // Tuyến tính
    dropout: Dropout, // Dropout
}

impl FeedForward {
    // Tạo một đối tượng FeedForward mới với các tham số đã cho
    pub fn new(hidden_size: usize, dropout: f64) -> Self {
        // Tạo một lớp tuyến tính mới với các tham số đã cho
        let linear1 = Linear::new(hidden_size, hidden_size, false);

        // Tạo một lớp tuyến tính mới với các tham số đã cho
        let linear2 = Linear::new(hidden_size, hidden_size, false);

        // Tạo một lớp dropout với tỉ lệ dropout đã cho
        let dropout = Dropout::new(dropout);

        Self {
            linear1,
            linear2,
            dropout,
        }
    }

    // Tính toán đầu ra của lan truyền tiến
    pub fn forward(&self, embeddings: &[Vec<f64>]) -> Vec<Vec<f64>> {
        // Tính toán đầu ra của lớp tuyến tính
        let embeddings = self.linear1.forward(embeddings);

        // Tính toán đầu ra của lớp dropout
        let embeddings = self.dropout.forward(&embeddings);

        // Tính toán đầu ra của lớp tuyến tính
        let embeddings = self.linear2.forward(&embeddings);

        // Trả về đầu ra của lan truyền tiến
        embeddings
    }
}

// Định nghĩa một lớp để thực hiện tuyến tính
#[derive(Clone, Debug)]
pub struct Linear {
    weights: Vec<Vec<f64>>, // Trọng số
    bias: Vec<f64>,         // Sai số
}

impl Linear {
    // Tạo một đối tượng Linear mới với các tham số đã cho
    pub fn new(input_size: usize, output_size: usize, bias: bool) -> Self {
        // Tạo một vector để lưu trữ các trọng số
        let mut weights: Vec<Vec<f64>> = Vec::new();

        // Tạo các trọng số với phân phối chuẩn
        let mut rng = rand::thread_rng();
        for _ in 0..output_size {
            let mut weight: Vec<f64> = Vec::new();
            for _ in 0..input_size {
                let w: f64 = rng.gen();
                weight.push(w);
            }
            weights.push(weight);
        }

        // Tạo một vector để lưu trữ các sai số
        let mut bias: Vec<f64> = Vec::new();

        // Nếu có sai số
        if bias.is_empty() {
            // Tạo các sai số với phân phối chuẩn
            for _ in 0..output_size {
                let b: f64 = rng.gen();
                bias.push(b);
            }
        }

        Self { weights, bias }
    }

    // Tính toán đầu ra của tuyến tính
    pub fn forward(&self, inputs: &[Vec<f64>]) -> Vec<Vec<f64>> {
        // Tính toán đầu ra của tuyến tính
        let mut outputs: Vec<Vec<f64>> = Vec::new();
        for i in 0..inputs.len() {
            let mut output: Vec<f64> = Vec::new();
            for j in 0..self.weights.len() {
                let mut sum: f64 = 0.0;
                for k in 0..self.weights[j].len() {
                    sum += inputs[i][k] * self.weights[j][k];
                }
                if self.bias.len() > 0 {
                    sum += self.bias[j];
                }
                output.push(sum);
            }
            outputs.push(output);
        }

        // Trả về đầu ra của tuyến tính
        outputs
    }
}

// Định nghĩa một lớp để thực hiện dropout
#[derive(Clone, Debug)]
pub struct Dropout {
    rate: f64, // Tỉ lệ dropout
}

impl Dropout {
    // Tạo một đối tượng Dropout mới với tỉ lệ dropout đã cho
    pub fn new(rate: f64) -> Self {
        Self { rate }
    }

    // Tính toán đầu ra của dropout
    pub fn forward(&self, inputs: &[Vec<f64>]) -> Vec<Vec<f64>> {
        // Tạo một vector để lưu trữ các vector đầu ra
        let mut outputs: Vec<Vec<f64>> = Vec::new();

        // Duyệt qua mỗi vector đầu vào
        for input in inputs.iter() {
            // Tạo một vector để lưu trữ vector đầu ra
            let mut output: Vec<f64> = Vec::new();

            // Duyệt qua mỗi phần tử trong vector đầu vào
            for i in 0..input.len() {
                // Nếu tỉ lệ dropout nhỏ hơn tỉ lệ dropout đã cho
                if rand::thread_rng().gen::<f64>() < self.rate {
                    // Thêm 0 vào vector đầu ra
                    output.push(0.0);
                } else {
                    // Nếu không, thêm phần tử vào vector đầu ra
                    output.push(input[i]);
                }
            }

            // Thêm vector đầu ra vào vector
            outputs.push(output);
        }

        // Trả về đầu ra của dropout
        outputs
    }
}
