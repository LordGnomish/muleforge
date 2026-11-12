package generated.routes;

import org.apache.camel.builder.RouteBuilder;
import org.apache.camel.LoggingLevel;
import jakarta.enterprise.context.ApplicationScoped;

@ApplicationScoped
public class OrderIngestRoute extends RouteBuilder {

    @Override
    public void configure() throws Exception {
        from("kafka:orders.raw?brokers={{kafka.brokers}}&groupId=order-ingest-group")
            .routeId("orderIngest")
            .log(LoggingLevel.INFO, "Received order: ${body}")
            .to("kafka:orders.validated?brokers={{kafka.brokers}}")
            ;
    }
}
