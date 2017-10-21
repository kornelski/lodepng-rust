use ffi::Error;
use std::mem;
use std::rc::Rc;

pub(crate) struct HuffmanTree {
    pub maxbitlen: usize,
    pub numcodes: usize,
    pub tree2d: Vec<u32>,
    pub tree1d: Vec<u32>,
    pub lengths: Vec<u32>,
}

impl HuffmanTree {
    pub fn new(numcodes: usize, lengths: Vec<u32>, maxbitlen: usize) -> Self {
        Self {
            numcodes,
            lengths,
            maxbitlen,
            tree2d: Vec::new(),
            tree1d: Vec::new(),
        }
    }

    pub fn code(&self, index: u32) -> u32 {
        self.tree1d[index as usize]
    }

    pub fn length(&self, index: u32) -> u32 {
        self.lengths[index as usize]
    }

    pub fn decode_symbol(&self, inp: &[u8], bp: &mut usize) -> Option<u32> {
        let inbitlength = inp.len() * 8;
        let mut treepos = 0;
        loop {
            /*error: end of input memory reached without endcode*/
            if *bp >= inbitlength {
                return None;
            }
            /*
                decode the symbol from the tree. The "readBitFromStream" code is inlined in
                the expression below because this is the biggest bottleneck while decoding
                */
            let ct = self.tree2d[(treepos << 1) + (((inp[*bp >> 3]) >> (*bp & 7)) & 1u8) as usize]; /*the symbol is decoded, return it*/
            (*bp) += 1; /*symbol not yet decoded, instead move tree position*/
            if (ct as usize) < self.numcodes {
                return Some(ct);
            } else {
                treepos = ct as usize - self.numcodes;
            }
            if treepos >= self.numcodes {
                return None;
            }
        }
    }


    fn new_2d_tree(tree: &mut HuffmanTree) -> Result<(), Error> {
        let mut nodefilled = 0;
        let mut treepos = 0;
        tree.tree2d = vec![32767; tree.numcodes as usize * 2];
        for n in 0..tree.numcodes as usize {
            for i in 0..tree.lengths[n] as isize {
                let bit = ((tree.tree1d[n] >> (tree.lengths[n] as isize - i - 1)) & 1) as usize;
                if treepos > 2147483647 || treepos + 2 > tree.numcodes {
                    return Err(Error(55));
                }
                if tree.tree2d[2 * treepos + bit as usize] == 32767 {
                    if i + 1 == tree.lengths[n] as isize {
                        tree.tree2d[2 * treepos + bit] = n as u32;
                        treepos = 0;
                    } else {
                        nodefilled += 1;
                        tree.tree2d[2 * treepos + bit] = (nodefilled + tree.numcodes) as u32;
                        treepos = nodefilled;
                    };
                } else {
                    let pos = tree.tree2d[2 * treepos + bit] as usize;
                    if pos < tree.numcodes {
                        return Err(Error(55));
                    }
                    treepos = pos - tree.numcodes;
                };
            }
        }
        for n in 0..(tree.numcodes * 2) {
            if tree.tree2d[n] == 32767 {
                tree.tree2d[n] = 0;
            }
        }
        Ok(())
    }

    fn from_lengths2(tree: &mut HuffmanTree) -> Result<(), Error> {
        let mut blcount: Vec<u32> = vec![0; tree.maxbitlen + 1];
        let mut nextcode: Vec<u32> = vec![0; tree.maxbitlen + 1];
        tree.tree1d = vec![0; tree.numcodes];

        let mut bits = 0;
        while bits != tree.numcodes {
            blcount[tree.lengths[bits] as usize] += 1;
            bits += 1
        }
        bits = 1;
        while bits <= tree.maxbitlen {
            nextcode[bits] = (nextcode[bits - 1] + blcount[bits - 1]) << 1;
            bits += 1
        }
        for n in 0..tree.numcodes {
            if tree.lengths[n] != 0 {
                tree.tree1d[n] = nextcode[tree.lengths[n as usize] as usize];
                nextcode[tree.lengths[n as usize] as usize] += 1;
            }
        }
        Self::new_2d_tree(tree)
    }

    pub fn from_lengths(bitlen: &[u32], maxbitlen: usize) -> Result<Self, Error> {
        let mut tree = Self::new(bitlen.len(), bitlen.to_owned(), maxbitlen);
        Self::from_lengths2(&mut tree)?;
        Ok(tree)
    }

    pub fn from_frequencies(frequencies: &[u32], mincodes: usize, maxbitlen: u32) -> Result<Self, Error> {
        let mut numcodes = frequencies.len();
        while frequencies[numcodes - 1] == 0 && numcodes > mincodes {
            numcodes -= 1;
        }
        let mut tree = Self::new(numcodes, huffman_code_lengths(frequencies, maxbitlen)?, maxbitlen as usize);
        Self::from_lengths2(&mut tree)?;
        Ok(tree)
    }
}

#[derive(Clone)]
struct BPMNode {
    pub weight: i32,
    /*index of this leaf node (called "count" in the paper)*/
    pub index: u32,
    pub tail: Option<Rc<BPMNode>>,
}

/*lists of chains*/
struct BPMLists {
    pub chains0: Vec<Rc<BPMNode>>,
    pub chains1: Vec<Rc<BPMNode>>,
}


pub(crate) fn huffman_code_lengths(frequencies: &[u32], maxbitlen: u32) -> Result<Vec<u32>, Error> {
    let numcodes = frequencies.len();
    if numcodes == 0 {
        return Err(Error(80)); /*error: a tree of 0 symbols is not supposed to be made*/
    } /*error: represent all symbols*/
    if (1 << maxbitlen) < numcodes {
        return Err(Error(80));
    }
    let mut leaves = vec![];
    for i in 0..numcodes {
        if frequencies[i] > 0 {
            leaves.push(BPMNode {
                weight: frequencies[i] as i32,
                index: i as u32,
                tail: None,
            });
        };
    }
    let mut lengths = vec![0; numcodes];
    /*ensure at least two present symbols. There should be at least one symbol
      according to RFC 1951 section 3.2.7. Some decoders incorrectly require two. To
      make these work as well ensure there are at least two symbols. The
      Package-Merge code below also doesn't work correctly if there's only one
      symbol, it'd give it the theoritical 0 bits but in practice zlib wants 1 bit*/
    if leaves.is_empty() {
        lengths[0] = 1;
        lengths[1] = 1;
    /*note that for RFC 1951 section 3.2.7, only lengths[0] = 1 is needed*/
    } else if leaves.len() == 1 {
        lengths[leaves[0].index as usize] = 1;
        lengths[(if leaves[0].index == 0 { 1 } else { 0 })] = 1;
    } else {
        let listsize = maxbitlen;
        leaves.sort_by_key(|a| a.weight);
        let mut lists = BPMLists {
            chains0: vec![BPMNode::new(leaves[0].weight, 1, None); listsize as usize],
            chains1: vec![BPMNode::new(leaves[1].weight, 2, None); listsize as usize],
        };

        /*each boundary_pm call adds one chain to the last list, and we need 2 * numpresent - 2 chains.*/
        for i in 2..(2 * leaves.len() - 2) {
            boundary_pm(&mut lists, &leaves, maxbitlen as usize - 1, i);
        }
        let mut next_node = Some(&lists.chains1[maxbitlen as usize - 1]);
        while let Some(node) = next_node {
            for leaf in &leaves[0..node.index as usize] {
                lengths[leaf.index as usize] += 1;
            }
            next_node = node.tail.as_ref();
        }
    }
    Ok(lengths)
}

impl BPMNode {
    /*creates a new chain node with the given parameters, from the memory in the lists */
    fn new(weight: i32, index: u32, tail: Option<Rc<BPMNode>>) -> Rc<Self> {
        Rc::new(BPMNode {
            weight,
            index,
            tail,
        })
    }
}


/*Boundary Package Merge step, numpresent is the amount of leaves, and c is the current chain.*/
fn boundary_pm(lists: &mut BPMLists, leaves: &[BPMNode], c: usize, num: usize) {
    let lastindex = lists.chains1[c].index as usize; /*sum of the weights of the head nodes of the previous lookahead chains.*/
    if c == 0 {
        if lastindex >= leaves.len() {
            return;
        }
        mem::swap(&mut lists.chains0[0], &mut lists.chains1[0]); // micro-optimization avoids bumping refcount
        lists.chains1[0] = BPMNode::new(leaves[lastindex].weight, lastindex as u32 + 1, None);
    } else {
        let sum = lists.chains0[c - 1].weight + lists.chains1[c - 1].weight;
        if lastindex < leaves.len() && sum > leaves[lastindex].weight {
            let w = leaves[lastindex].weight;
            let t = lists.chains1[c].tail.clone();
            mem::swap(&mut lists.chains0[c], &mut lists.chains1[c]);
            lists.chains1[c] = BPMNode::new(w, lastindex as u32 + 1, t);
            return;
        }
        let t = Rc::clone(&lists.chains1[c - 1]);
        mem::swap(&mut lists.chains0[c], &mut lists.chains1[c]);
        lists.chains1[c] = BPMNode::new(sum, lastindex as u32, Some(t));
        /*in the end we are only interested in the chain of the last list, so no
            need to recurse if we're at the last one (this gives measurable speedup)*/
        if num + 1 < (2 * leaves.len() - 2) {
            boundary_pm(lists, leaves, c - 1, num);
            boundary_pm(lists, leaves, c - 1, num);
        };
    };
}
