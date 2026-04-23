//! K8s emitter: generates Kubernetes manifests (Deployment, Service, ConfigMap)
//! with Kustomize overlays.

use std::path::Path;

use crate::ast::camel_ir::CamelProject;
use crate::Result;

pub fn emit(ir: &CamelProject, output_dir: &Path) -> Result<()> {
    let base_dir = output_dir.join("k8s/base");
    std::fs::create_dir_all(&base_dir)?;

    let name = &ir.name;

    // Deployment
    let deployment = format!(
        r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {name}
  labels:
    app: {name}
    generated-by: muleforge
spec:
  replicas: 1
  selector:
    matchLabels:
      app: {name}
  template:
    metadata:
      labels:
        app: {name}
    spec:
      containers:
        - name: {name}
          image: {name}:latest
          ports:
            - containerPort: 8080
              name: http
              protocol: TCP
          env:
            - name: JAVA_OPTS
              value: "-Xmx512m -Xms256m"
          livenessProbe:
            httpGet:
              path: /q/health/live
              port: 8080
            initialDelaySeconds: 30
            periodSeconds: 10
          readinessProbe:
            httpGet:
              path: /q/health/ready
              port: 8080
            initialDelaySeconds: 5
            periodSeconds: 5
          resources:
            requests:
              cpu: 250m
              memory: 256Mi
            limits:
              cpu: "1"
              memory: 512Mi
"#
    );

    // Service
    let service = format!(
        r#"apiVersion: v1
kind: Service
metadata:
  name: {name}
  labels:
    app: {name}
spec:
  type: ClusterIP
  ports:
    - port: 8080
      targetPort: 8080
      protocol: TCP
      name: http
  selector:
    app: {name}
"#
    );

    // ConfigMap
    let configmap = format!(
        r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: {name}-config
  labels:
    app: {name}
data:
  APPLICATION_NAME: "{name}"
"#
    );

    // Kustomization
    let kustomization = format!(
        r#"apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
commonLabels:
  app: {name}
resources:
  - deployment.yaml
  - service.yaml
  - configmap.yaml
"#
    );

    std::fs::write(base_dir.join("deployment.yaml"), deployment)?;
    std::fs::write(base_dir.join("service.yaml"), service)?;
    std::fs::write(base_dir.join("configmap.yaml"), configmap)?;
    std::fs::write(base_dir.join("kustomization.yaml"), kustomization)?;

    // Dev overlay
    let dev_dir = output_dir.join("k8s/overlays/dev");
    std::fs::create_dir_all(&dev_dir)?;
    let dev_kustomization = r#"apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
bases:
  - ../../base
patchesStrategicMerge:
  - deployment-patch.yaml
"#
    .to_string();
    let dev_patch = format!(
        r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {name}
spec:
  replicas: 1
  template:
    spec:
      containers:
        - name: {name}
          resources:
            requests:
              cpu: 100m
              memory: 128Mi
            limits:
              cpu: 500m
              memory: 256Mi
"#
    );
    std::fs::write(dev_dir.join("kustomization.yaml"), dev_kustomization)?;
    std::fs::write(dev_dir.join("deployment-patch.yaml"), dev_patch)?;

    // Prod overlay
    let prod_dir = output_dir.join("k8s/overlays/prod");
    std::fs::create_dir_all(&prod_dir)?;
    let prod_kustomization = r#"apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
bases:
  - ../../base
patchesStrategicMerge:
  - deployment-patch.yaml
  - hpa.yaml
"#
    .to_string();
    let prod_patch = format!(
        r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {name}
spec:
  replicas: 3
  template:
    spec:
      containers:
        - name: {name}
          resources:
            requests:
              cpu: 500m
              memory: 512Mi
            limits:
              cpu: "2"
              memory: 1Gi
"#
    );
    let hpa = format!(
        r#"apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: {name}
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: {name}
  minReplicas: 3
  maxReplicas: 10
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: 70
"#
    );
    std::fs::write(prod_dir.join("kustomization.yaml"), prod_kustomization)?;
    std::fs::write(prod_dir.join("deployment-patch.yaml"), prod_patch)?;
    std::fs::write(prod_dir.join("hpa.yaml"), hpa)?;

    Ok(())
}
