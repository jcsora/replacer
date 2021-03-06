use bytes::BytesMut;
use rand::Rng;
use serde_derive::Deserialize;
use std::collections::HashMap;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Deserialize)]
struct Config {
    pat: Option<String>,
    to: Option<Vec<(String, u8)>>,
}

#[derive(Debug)]
struct RepStr {
    s: String,
    w_l: u8,
    w_r: u8,
}

pub async fn read_chunk(
    path: String,
    tx: mpsc::Sender<String>,
    l_tx: oneshot::Sender<HashMap<String, u64>>,
) {
    //替换过程的配置
    let pat: String;
    let rep_str: Vec<RepStr>;
    let mut rep_log: HashMap<String, u64>;
    match File::open("Config.toml").await {
        Ok(mut config) => {
            let mut conf_buf: Vec<u8> = Vec::new();
            let n = config.read_to_end(&mut conf_buf).await.unwrap();
            if n == 0 {
                println!("Config.toml 配置错误");
                std::process::exit(64);
            }
            let rep_conf = build_rep_conf(conf_buf);
            pat = rep_conf.0;
            rep_str = rep_conf.1;
            rep_log = rep_conf.2;
        }
        Err(_) => {
            println!("未找到 Config.toml");
            std::process::exit(64);
        }
    }

    //要替换的源文件
    let mut file = File::open(path).await.unwrap();
    let mut buf = BytesMut::with_capacity(16 * 1024);
    let mut n: usize;
    let mut chunk_tail: String = "".to_string();
    let mut chunk: String;

    loop {
        n = file.read_buf(&mut buf).await.unwrap();
        if n == 0 {
            if chunk_tail.len() != 0 {
                tx.send(chunk_tail.clone()).await.unwrap();
            }
            break;
        }
        chunk = chunk_tail.clone() + &String::from_utf8(buf.to_vec()).unwrap();
        chunk_tail.clear();
        match chunk.rfind(&pat) {
            Some(mut pos) => {
                //将最后一个待替换字符串之后的内容存到下次
                pos = pos + pat.len();
                let (first, last) = chunk.split_at_mut(pos);
                chunk_tail = last.to_string();
                chunk = first.to_string();
                //改变buf大小，下次少读取一些
                buf.resize(buf.capacity() - pat.len(), 0x0);
                buf.clear();
            }
            None => {
                //若本次读取的buffer内没有待替换的字符串,则直接写入
                tx.send(chunk.clone()).await.unwrap();
                buf.clear();
                continue;
            }
        }
        {
            let mut rng = rand::thread_rng();
            let mut rand: u8;
            'outer: loop {
                rand = rng.gen_range(1..=100);
                for rep in rep_str.iter() {
                    if rand >= rep.w_l || rand <= rep.w_r {
                        let chunk_tmp: String = chunk.replacen(&pat, &rep.s, 1);
                        if chunk == chunk_tmp {
                            break 'outer;
                        } else {
                            if let Some(n) = rep_log.get_mut(&rep.s) {
                                *n = *n + 1;
                            }
                            chunk = chunk_tmp;
                        }
                    }
                }
            }
        }
        tx.send(chunk.clone()).await.unwrap();
    }
    l_tx.send(rep_log).unwrap();
}

fn build_rep_conf(v: Vec<u8>) -> (String, Vec<RepStr>, HashMap<String, u64>) {
    let config: Config = toml::from_slice(&v[..]).unwrap();
    let mut to: Vec<RepStr> = Vec::new();
    let mut rep_log: HashMap<String, u64> = HashMap::new();
    let mut w_pos: u8 = 1;

    if let None = config.pat {
        println!("Config.toml 内未设置 pat 项");
        std::process::exit(64);
    }
    if let Some(t) = config.to {
        for (s, w) in t.iter() {
            to.push(RepStr {
                s: s.to_string(),
                w_l: w_pos,
                w_r: w_pos + w - 1,
            });
            rep_log.insert(s.to_string(), 0);
            w_pos += w;
        }
        if w_pos != 101 {
            println!("Config.toml 内的 to 总概率必须为 100");
            std::process::exit(64);
        }
    }
    (config.pat.unwrap(), to, rep_log)
}
