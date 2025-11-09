use std::borrow::Cow;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use ndarray::{Array2, Array3, Ix3};
use ort::execution_providers::{CUDAExecutionProvider, ExecutionProvider};
use ort::session::{Session, SessionInputValue};
use ort::value::Tensor;

pub struct OnnxEngine {
    session: Mutex<Session>,
    use_gpu: bool,
}

impl OnnxEngine {
    pub fn new(model_path: impl AsRef<Path>, use_gpu: bool) -> Result<Self> {
        let model_path = model_path.as_ref();
        let mut builder = Session::builder().context("Failed to create ONNX session builder")?;

        let mut gpu_active = false;

        if use_gpu {
            let cuda = CUDAExecutionProvider::default();
            match cuda.is_available() {
                Ok(true) => {
                    let provider = cuda.build();
                    builder = builder
                        .with_execution_providers([provider])
                        .context("Failed to attach CUDA execution provider")?;
                    gpu_active = true;
                }
                Ok(false) => {
                    println!("CUDA execution provider unavailable; falling back to CPU execution");
                }
                Err(err) => {
                    println!(
                        "CUDA availability check failed ({}); falling back to CPU execution",
                        err
                    );
                }
            }
        }

        let session = builder
            .commit_from_file(model_path)
            .with_context(|| format!("Failed to load ONNX model {}", model_path.display()))?;

        Ok(Self {
            session: Mutex::new(session),
            use_gpu: gpu_active,
        })
    }

    pub fn run(
        &self,
        input_ids: &Array2<i64>,
        attention_mask: Option<&Array2<i64>>,
        token_type_ids: Option<&Array2<i64>>,
    ) -> Result<Array3<f32>> {
        let mut session = self.session.lock().expect("ONNX session mutex poisoned");
        run_session(&mut session, input_ids, attention_mask, token_type_ids)
    }

    pub fn uses_gpu(&self) -> bool {
        self.use_gpu
    }
}

fn run_session(
    session: &mut Session,
    input_ids: &Array2<i64>,
    attention_mask: Option<&Array2<i64>>,
    token_type_ids: Option<&Array2<i64>>,
) -> Result<Array3<f32>> {
    let mut inputs: Vec<(Cow<'static, str>, SessionInputValue)> = Vec::new();

    let input_tensor = Tensor::from_array(input_ids.clone())?;
    inputs.push(("input_ids".into(), input_tensor.into()));

    if let Some(mask) = attention_mask {
        let mask_tensor = Tensor::from_array(mask.clone())?;
        inputs.push(("attention_mask".into(), mask_tensor.into()));
    }

    if let Some(types) = token_type_ids {
        let token_tensor = Tensor::from_array(types.clone())?;
        inputs.push(("token_type_ids".into(), token_tensor.into()));
    }

    let mut outputs = session
        .run(inputs)
        .context("ONNX inference execution failed")?;

    let logits_value = outputs
        .remove("logits")
        .context("ONNX outputs missing 'logits' tensor")?;

    let logits = logits_value
        .try_extract_array::<f32>()
        .context("Failed to extract logits tensor")?
        .into_dimensionality::<Ix3>()
        .context("Logits tensor does not have rank 3")?
        .into_owned();

    Ok(logits)
}
