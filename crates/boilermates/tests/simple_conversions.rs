use boilermates::boilermates;

#[test]
fn correct_conversion() {

  #[boilermates("Step2", "Step3")]
  #[boilermates(attr_for("Step2", "#[derive(Clone)]"))]
  #[derive(Clone)]
  struct Step1 {
    field1: String,
    #[boilermates(not_in("Step1"))]
    field2: u32,
    #[boilermates(only_in("Step3"))]
    field3: f64,
  }

  let step1 = Step1 { field1: "test".into() };

  let step2 = step1.clone().into_step2(42);
  assert_eq!(step2.field1, step1.field1);

  let step3 = step2.clone().into_step3(42.0);
  assert_eq!(step3.field1, step1.field1);
  assert_eq!(step3.field2, step2.field2);
  assert_eq!(step3.field3, 42.0);
}