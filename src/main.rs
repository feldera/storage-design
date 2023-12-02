#![allow(unused)]
use clap::{Parser, ValueEnum};
use std::fmt::{Display, Formatter, Result as FmtResult};

const TB: u64 = 1 << 40;
const GB: u64 = 1 << 30;
const MB: u64 = 1 << 20;
const KB: u64 = 1 << 10;

#[derive(Clone)]
struct Params {
    // Total size of all of the data stored in the file, in bytes.
    total_data_size: u64,

    /// Size of each individual value in the file, in bytes.  There are
    /// `total_data_size / value_size` of these values.
    value_size: u64,

    /// Minimum size of a data block, in bytes.  This should be a power of 2,
    /// 4096 or greater, probably no more than a few megabytes.
    min_data_block: u64,

    /// Minimum size of an index block, in bytes.  This should be a power of 2,
    /// 4096 or greater, probably no more than a few megabytes.
    min_index_block: u64,

    /// Minimum branching factor.  This should be at least 4 and probably no
    /// more than 100 or so.
    min_branch: u64,
}

impl Params {
    /// Returns the total number of values stored in the file.
    fn total_values(&self) -> u64 {
        self.total_data_size / self.value_size
    }
}

struct Index {
    params: Params,
    index_type: IndexType,

    /// Size of each index entry in bytes.
    index_entry_size: u64,

    /// Number of `index_entry_size` items that fit in an index block.
    entries_per_block: u64,

    /// Size of an index block.
    block_size: u64,

    /// `coverage[0]` is the number of values covered by a level-1 index block,
    /// that is, `values_per_data_block * entries_per_index_block`.
    ///
    /// `coverage[1]` is the number of values covered by a level-2 index block,
    /// that is, `entries_per_index_block * coverage[0]`.
    ///
    /// There are as many elements as necessary so that the final element is
    /// greater than or equal to `params.total_values()`.
    coverage: Vec<u64>,

    /// Height of the index.  Same as `coverage.len()`.
    height: usize,
}

impl Index {
    fn new(
        params: &Params,
        index_type: IndexType,
        index_entry_size: u64,
        values_per_data_block: u64,
    ) -> Self {
        let params = params.clone();

        let entries_per_index_block =
            (params.min_index_block / index_entry_size).max(params.min_branch);
        let index_block_size = index_entry_size * entries_per_index_block;

        let mut coverage = Vec::new();
        loop {
            let last = coverage.last().copied().unwrap_or(values_per_data_block);
            if last >= params.total_values() {
                break;
            }
            coverage.push(last * entries_per_index_block);
        }
        let height = coverage.len();

        Index {
            params,
            index_type,
            index_entry_size,
            entries_per_block: entries_per_index_block,
            block_size: index_block_size,
            coverage,
            height,
        }
    }

    /// Returns the number of bytes in the index, across all levels of the
    /// index.
    fn total_size(&self) -> u64 {
        let total_values = self.params.total_values();
        let total_index_blocks: u64 = self
            .coverage
            .iter()
            .map(
                // Calculate number of index blocks at this level.
                |&coverage| {
                    let quotient = total_values / coverage;
                    let remainder = total_values % coverage > 0;
                    if remainder {
                        quotient + 1
                    } else {
                        quotient
                    }
                },
            )
            .sum();
        total_index_blocks * self.block_size
    }
}

struct LayerFile {
    params: Params,

    /// Number of data values that fit in a data block.
    values_per_data_block: u64,

    /// Size of a data block.
    data_block_size: u64,

    /// Number of data blocks to fill up `TOTAL_DATA_SIZE`.
    total_data_blocks: u64,

    indexes: Vec<Index>,
}

impl LayerFile {
    fn new(params: &Params) -> Self {
        let values_per_data_block =
            (params.min_data_block / params.value_size).max(params.min_branch);
        let data_block_size = params.value_size * values_per_data_block;
        let total_data_blocks = params.total_data_size / data_block_size;

        // Each entry in the data index contains two values (first and last in
        // the child block).
        let data_index = Index::new(
            params,
            IndexType::Data,
            2 * params.value_size,
            values_per_data_block,
        );

        // The row index in column 1 contains the child block's offset, size,
        // and whether it is an index or data block.  6 bytes is enough.
        let c1row_index = Index::new(params, IndexType::C1Row, 6, values_per_data_block);

        // The row index in other columns also needs the child's starting row
        // number.
        let row_index = Index::new(params, IndexType::Row, 12, values_per_data_block);

        let filter_index = Index::new(params, IndexType::Filter, 5, 65536);

        Self {
            params: params.clone(),
            values_per_data_block,
            data_block_size,
            total_data_blocks,
            indexes: vec![data_index, c1row_index, row_index, filter_index],
        }
    }
}

struct HumanBytes(u64);
impl Display for HumanBytes {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        let s = if self.0 >= TB {
            format!("{} TB", self.0 / TB)
        } else if self.0 >= 10 * GB - GB / 10 {
            format!("{} GB", self.0 / GB)
        } else if self.0 >= GB {
            format!("{:.1} GB", self.0 as f64 / GB as f64)
        } else if self.0 >= 10 * MB - MB / 10 {
            format!("{} MB", self.0 / MB)
        } else if self.0 >= MB {
            format!("{:.1} MB", self.0 as f64 / MB as f64)
        } else if self.0 >= KB {
            format!("{} kB", self.0 / KB)
        } else {
            format!("{}", self.0)
        };
        write!(f, "{s:>width$}", width = f.width().unwrap_or_default())
    }
}

struct HumanCount(u64);
impl Display for HumanCount {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        const QUADRILLION: u64 = 1_000_000_000_000_000;
        const TRILLION: u64 = 1_000_000_000_000;
        const BILLION: u64 = 1_000_000_000;
        const MILLION: u64 = 1_000_000;
        const THOUSAND: u64 = 1_000;
        let s = if self.0 >= QUADRILLION {
            format!("{} Q", self.0 / QUADRILLION)
        } else if self.0 >= TRILLION {
            format!("{} T", self.0 / TRILLION)
        } else if self.0 >= BILLION {
            format!("{} B", self.0 / BILLION)
        } else if self.0 >= MILLION {
            format!("{} M", self.0 / MILLION)
        } else if self.0 >= THOUSAND {
            format!("{} k", self.0 / THOUSAND)
        } else {
            format!("{}", self.0)
        };
        write!(f, "{s:>width$}", width = f.width().unwrap_or_default())
    }
}

#[derive(Parser, Debug)]
struct Args {
    /// Minimum branching factor in data and index blocks.
    #[clap(long, default_value_t = 32)]
    min_branch: u64,

    /// Minimum data block size, in bytes.
    #[clap(long, default_value_t = 8192)]
    min_data_block: u64,

    /// Minimum index block size, in bytes.
    #[clap(long, default_value_t = 8192)]
    min_index_block: u64,

    /// Total data size, as a power of 2 exponent, e.g. 30 for 1 GB, 37 for 128
    /// GB, 40 for 1 TB.
    #[clap(long, default_value_t = 40)]
    total_data_size: u32,

    /// Index(es) to include.
    #[clap(long="index", default_values_t = vec![IndexType::Data, IndexType::C1Row, IndexType::Row, IndexType::Filter])]
    indexes: Vec<IndexType>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum IndexType {
    /// Data values.
    Data,

    /// Column 1 row number.
    #[clap(alias = "c1row")]
    C1Row,

    /// Row number in columns other than 1.
    Row,

    /// Filter.
    Filter,
}

impl Display for IndexType {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        let s = match self {
            IndexType::Data => "data",
            IndexType::C1Row => "c1row",
            IndexType::Row => "row",
            IndexType::Filter => "filter",
        };
        write!(f, "{s:>width$}", width = f.width().unwrap_or_default())
    }
}

fn main() {
    let Args {
        min_branch,
        min_data_block,
        min_index_block,
        total_data_size,
        indexes,
    } = Args::parse();

    let total_data_size = 1 << total_data_size;

    println!("Index coverage for {} data, min_branch={min_branch}, min_data_block={min_data_block}, min_index_block={min_index_block}:",
             HumanBytes(total_data_size));
    print!(
        r#"
         # of   Values        Entries            # of values covered by a single index block
 Value  Values   /Data         /Index  Index   -----------------------------------------------   Index
  Size  in 1TB   Block  Index   Block  Height    L1     L2     L3     L4     L5     L6     L7     Size
------  ------  ------  -----  ------  ------  -----  -----  -----  -----  -----  -----  -----  ------
"#
    );
    for value_size in (4..=16).map(|shift| 1 << shift) {
        let params = Params {
            total_data_size,
            value_size,
            min_data_block,
            min_index_block,
            min_branch,
        };
        let layer_file = LayerFile::new(&params);
        for (i, index) in layer_file
            .indexes
            .iter()
            .filter(|index| indexes.iter().find(|t| **t == index.index_type).is_some())
            .enumerate()
        {
            if i == 0 {
                print!(
                    "{:5}  {:7}  {:6}",
                    HumanBytes(value_size),
                    HumanCount(layer_file.params.total_values()),
                    layer_file.values_per_data_block
                );
            } else {
                print!("{:5}  {:7}  {:6}", "", "", "");
            }
            print!(
                "  {:6} {:6}  {:6}",
                index.index_type, index.entries_per_block, index.height
            );
            for &coverage in &index.coverage {
                print!("  {:5}", HumanCount(coverage));
            }
            for _ in index.height..7 {
                print!("       ");
            }
            println!("  {:6}", HumanBytes(index.total_size()));
        }
    }
}
