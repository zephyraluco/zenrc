use std::slice;
use std::sync::Arc;
use arrow::array::{Float32Array, StringArray, UInt32Array, UInt64Array, ListArray, RecordBatch};
use arrow::datatypes::{DataType, Field, Fields, Float32Type, Schema};
use arrow::ipc::writer::StreamWriter;
use zenrc_shm::shm::MemoryHandle;
use std::io::Cursor;

fn main() -> anyhow::Result<()> {
    // 共享内存
    let name = "/my_shared_mem_arrow";
    let size: usize = 4096 * 64;
    let mut mem_handle = MemoryHandle::new(name, size).expect("MemoryHandle::new failed");

    // ---------------------------
    // 模拟 LaserScan 数据
    // ---------------------------
    let mut seq = 245911u32;
    let stamp_secs = 1730098366u64;
    let stamp_nsecs = 344599000u32;
    let frame_id = "laser_link";

    let ranges: Vec<f32> = vec![
        10.72188, 10.750879, 10.75988, 10.767879, 10.76188, 10.76388,
        10.75988, 10.76388, 10.76688, 10.77688, 10.77888, 10.78788, f32::NAN,
        3.867879, 3.847879, 3.836879, 3.837879, 3.833879, 3.830879,
        3.831879, 3.831879, 3.83288, 3.83288, 3.833879, 3.828879,
    ];

    // ---------------------------
    // Arrow Schema 定义
    // ---------------------------
    let schema = Arc::new(Schema::new(vec![
        Field::new("seq", DataType::UInt32, false),
        Field::new("stamp_secs", DataType::UInt64, false),
        Field::new("stamp_nsecs", DataType::UInt32, false),
        Field::new("frame_id", DataType::Utf8, false),
        Field::new("angle_min", DataType::Float32, false),
        Field::new("angle_max", DataType::Float32, false),
        Field::new("angle_increment", DataType::Float32, false),
        Field::new("time_increment", DataType::Float32, false),
        Field::new("scan_time", DataType::Float32, false),
        Field::new("range_min", DataType::Float32, false),
        Field::new("range_max", DataType::Float32, false),
        Field::new(
            "ranges",
            DataType::List(Arc::new(Field::new("item", DataType::Float32, true))),
            false,
        ),
    ]));
loop {
    // ---------------------------
    // 构造 RecordBatch
    // ---------------------------
	seq +=1;
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(UInt32Array::from(vec![seq])),
            Arc::new(UInt64Array::from(vec![stamp_secs])),
            Arc::new(UInt32Array::from(vec![stamp_nsecs])),
            Arc::new(StringArray::from(vec![frame_id])),
            Arc::new(Float32Array::from(vec![-1.7453293])),
            Arc::new(Float32Array::from(vec![1.5707964])),
            Arc::new(Float32Array::from(vec![0.00436325])),
            Arc::new(Float32Array::from(vec![4.62963e-05])),
            Arc::new(Float32Array::from(vec![0.06666667])),
            Arc::new(Float32Array::from(vec![0.05])),
            Arc::new(Float32Array::from(vec![30.0])),
            Arc::new(ListArray::from_iter_primitive::<Float32Type, _, _>(vec![
                Some(ranges.clone().into_iter().map(Some).collect::<Vec<Option<f32>>>()),
            ])),
        ],
    )?;

    // ---------------------------
    // 写入 Arrow IPC 格式到内存缓冲区
    // ---------------------------
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = StreamWriter::try_new(&mut cursor, &schema)?;
        writer.write(&batch)?;
        writer.finish()?;
    }
    let data = cursor.into_inner();

    println!("Arrow encoded size: {} bytes", data.len());
    if data.len() > size {
        panic!("Shared memory too small for Arrow buffer");
    }

    // ---------------------------
    // 拷贝到共享内存
    // ---------------------------
    unsafe {
        let slice = slice::from_raw_parts_mut(mem_handle.get_mut_ptr().as_ptr(), data.len());
        slice.copy_from_slice(&data);
    }

    println!("✅ Arrow RecordBatch (LaserScan) 已写入共享内存");
    println!("前64字节: {:02X?}", &data[..64.min(data.len())]);
	
		std::thread::sleep(std::time::Duration::from_millis(1));
	}
    Ok(())
}
